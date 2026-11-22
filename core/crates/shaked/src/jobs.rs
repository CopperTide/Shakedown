//! Background jobs without a caller: system transactions (`Store::system_tx`).

use anyhow::Result;

use crate::{
    auth::{run_tokens::token_live, SystemScope},
    model::BranchStatus,
    store::Store,
};

/// What was running when shaked last stopped won't go on: branches are interrupted, built-in
/// suites and experiments get no more runs, and reviews won't finish. A branch a sandbox's worker
/// runs (it has a live worker token, counted in unpaused time: a long pause keeps it) goes on:
/// its worker reconnects to this shaked. It stays running, and so do the suites and experiments
/// it belongs to (a worker that never comes back is the sandbox reconciler's to end).
const INTERRUPT: &str = concat!(
    "
UPDATE branches b SET status = 'interrupted', ended_at = now() WHERE b.status = 'running'
 AND NOT EXISTS (SELECT 1 FROM run_tokens t WHERE t.branch_id = b.id AND t.kind = 'worker' AND ",
    token_live!(),
    ");
UPDATE suites SET ended_at = now(), error = 'shaked stopped before the suite finished'
 WHERE (runner <> 'external' OR connection IS NOT NULL) AND ended_at IS NULL
 AND NOT EXISTS (SELECT 1 FROM suite_runs sr JOIN branches b ON b.run_id = sr.run_id
                 WHERE sr.suite_id = suites.id AND b.status = 'running');
UPDATE experiments SET ended_at = now(), error = 'shaked stopped before the experiment finished'
 WHERE ended_at IS NULL
 AND NOT EXISTS (SELECT 1 FROM suites s JOIN suite_runs sr ON sr.suite_id = s.id
                 JOIN branches b ON b.run_id = sr.run_id
                 WHERE s.experiment_id = experiments.id AND b.status = 'running');
UPDATE reviews SET status = 'error', finished_at = now(), error = 'shaked stopped before the review finished'
 WHERE status = 'running';"
);

/// Marks what was running when shaked last stopped as interrupted (at start, before any run).
pub async fn interrupt_at_start(store: &Store) -> Result<()> {
    let tx = store.system_tx(SystemScope::interrupt_at_start()).await?;
    tx.batch(INTERRUPT).await?;
    tx.commit().await
}

/// A branch a sandbox's worker runs, as the link sweep finds it: its hold, and whether its
/// worker still has a live token.
#[derive(Debug, Clone, PartialEq)]
pub struct Linked {
    pub branch: String,
    pub hold: crate::model::Hold,
    pub token_live: bool,
}

/// Every running branch that ever had a worker token.
const LINKED: &str = concat!(
    "SELECT b.id, b.hold,
    EXISTS (SELECT 1 FROM run_tokens t WHERE t.branch_id = b.id AND t.kind = 'worker' AND ",
    token_live!(),
    ")
  FROM branches b WHERE b.status = 'running'
   AND EXISTS (SELECT 1 FROM run_tokens t WHERE t.branch_id = b.id AND t.kind = 'worker')"
);

/// The running branches sandboxes' workers run (a system job: every owner's).
pub async fn linked_branches(store: &Store) -> Result<Vec<Linked>> {
    let tx = store.system_tx(SystemScope::link_sweep()).await?;
    let rows = tx.query(LINKED, &[]).await?;
    tx.commit().await?;
    let linked = rows.iter().map(|r| Linked {
        branch: r.get(0),
        hold: crate::model::Hold::from_stored(r.get::<_, Option<String>>(1).as_deref()),
        token_live: r.get(2),
    });
    Ok(linked.collect())
}

/// Ends branch `$1` with status `$2` and error `$3` when it still runs (and, when `$4`, only if it
/// is unpaused and no worker token of it is live): its status after.
const END_RUNNING: &str = concat!(
    "WITH ended AS (
    UPDATE branches b SET status = $2, error = $3, ended_at = now()
     WHERE b.id = $1 AND b.status = 'running'
       AND (NOT $4 OR (b.hold IS DISTINCT FROM 'paused' AND NOT EXISTS (
         SELECT 1 FROM run_tokens t WHERE t.branch_id = b.id AND t.kind = 'worker' AND ",
    token_live!(),
    ")))
    RETURNING b.status)
  SELECT coalesce((SELECT status FROM ended), (SELECT status FROM branches WHERE id = $1))"
);

/// When [`end_linked`] ends a branch that still runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndIf {
    /// Always.
    Running,
    /// Only if its worker token died and it is not paused (checked as it ends).
    TokenDied,
}

/// Ends linked `branch` with `status` (and `error`) when it still runs (and `when` says so), and
/// revokes its tokens: how the branch ended (`status`, or what it had ended with before, or
/// running), None when there is no such branch. A system job: the control box ends it, whoever
/// owns it.
pub async fn end_linked(
    store: &Store,
    branch: &str,
    (status, error): (BranchStatus, Option<String>),
    when: EndIf,
) -> Result<Option<BranchStatus>> {
    let tx = store.system_tx(SystemScope::link_sweep()).await?;
    let only_dead = when == EndIf::TokenDied;
    let params: [&(dyn tokio_postgres::types::ToSql + Sync); 4] =
        [&branch, &status.as_str(), &error, &only_dead];
    let row = tx.query_one(END_RUNNING, &params).await?;
    let ended: Option<BranchStatus> = row
        .get::<_, Option<String>>(0)
        .as_deref()
        .and_then(BranchStatus::parse);
    if ended != Some(BranchStatus::Running) {
        let none: Option<&str> = None;
        tx.execute(crate::store::REVOKE, &[&branch, &none]).await?;
    }
    tx.commit().await?;
    Ok(ended)
}
