//! Background jobs without a caller: system transactions (`Store::system_tx`).

use anyhow::Result;

use crate::{auth::SystemScope, store::Store};

/// What was running when shaked last stopped won't go on: branches are interrupted, built-in
/// suites and experiments get no more runs, and reviews won't finish. A branch a sandbox's worker
/// runs (it has a live worker token) goes on: its worker reconnects to this shaked. It stays
/// running, and so do the suites and experiments it belongs to (a worker that never comes back
/// is the sandbox reconciler's to end).
const INTERRUPT: &str = "
UPDATE branches SET status = 'interrupted', ended_at = now() WHERE status = 'running'
 AND NOT EXISTS (SELECT 1 FROM run_tokens t WHERE t.branch_id = branches.id AND t.kind = 'worker'
                 AND t.revoked_at IS NULL AND t.expires_at > now());
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
 WHERE status = 'running';";

/// Marks what was running when shaked last stopped as interrupted (at start, before any run).
pub async fn interrupt_at_start(store: &Store) -> Result<()> {
    let tx = store.system_tx(SystemScope::interrupt_at_start()).await?;
    tx.batch(INTERRUPT).await?;
    tx.commit().await
}
