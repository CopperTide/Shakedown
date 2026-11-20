use std::{collections::HashMap, sync::Mutex};

use anyhow::{anyhow, Context, Result};
use deadpool_postgres::Pool;
use serde_json::Value;
use tokio::sync::{broadcast, Mutex as AsyncMutex};
use tokio_postgres::{types::ToSql, Row};

use crate::{
    auth::{Scope, SystemScope},
    model::*,
    overhead::Overhead,
    pg::Admin,
};
use parts::proto::Mode;

mod api_tokens;
mod connections;
mod experiments;
mod open;
mod reviews;
mod run_tokens;
mod seed;
mod suites;
mod tx;
mod usage;
pub use api_tokens::ApiTokenRow;
pub use connections::{is_secret, Connection, NewConnection, REDACTED};
pub use experiments::{NewExperiment, NewSetup};
pub use open::{prepare_db, template_key, DbUrls};
pub use reviews::{ReviewDone, StoredReview};
pub use seed::{LOCAL_ORG_ID, LOCAL_USER_ID};
pub use suites::NewSuite;
pub use tx::Tx;

pub type RunId = String;
pub type BranchId = String;

/// The database shaked keeps its records in.
pub const DEFAULT_DB: &str = "shakedown";
/// Its name until 2026-11-19, still used while the server has no [`DEFAULT_DB`].
pub const OLD_DEFAULT_DB: &str = "arena";

/// `SHAKEDOWN_DB` (`get` reads a setting), else None (the default database). Under
/// `SHAKEDOWN_TEST_GUARD`, without `SHAKEDOWN_ALLOW_DEFAULT_DB`, a default database is refused,
/// before anything connects.
pub fn db_setting(get: impl Fn(&str) -> Option<String>) -> Result<Option<String>> {
    let named = get("SHAKEDOWN_DB");
    let guarded =
        get("SHAKEDOWN_TEST_GUARD").is_some() && get("SHAKEDOWN_ALLOW_DEFAULT_DB").is_none();
    let default = named
        .as_deref()
        .is_none_or(|n| n == DEFAULT_DB || n == OLD_DEFAULT_DB);
    if guarded && default {
        anyhow::bail!(
            "SHAKEDOWN_TEST_GUARD is set: refusing the shared default database; set SHAKEDOWN_DB to a scratch database (or SHAKEDOWN_ALLOW_DEFAULT_DB=1)"
        );
    }
    Ok(named)
}

/// The database to keep the records in: `name` when it exists, else `old` when that exists (an
/// install from before the rename keeps its records), else `name` (to be made).
pub async fn pick_db(admin: &Admin, name: &str, old: &str) -> Result<String> {
    if !admin.exists(name).await? && admin.exists(old).await? {
        return Ok(old.to_string());
    }
    Ok(name.to_string())
}

const BRANCH_COLS: &str = "id, run_id, parent_id, start_seq, team, changes, fork_note, status, \
     error, created_at, ended_at, restore_mode, restored";
const EVENT_COLS: &str = "branch_id, seq, at, agent, kind, payload";
/// `BRANCH_COLS` of `b` in [`BRANCHES_OF_RUNS`].
const BRANCH_COLS_B: &str =
    "b.id, b.run_id, b.parent_id, b.start_seq, b.team, b.changes, b.fork_note, \
     b.status, b.error, b.created_at, b.ended_at, b.restore_mode, b.restored";
/// Branches joined to their runs: under row-level security a branch of a run the scope cannot
/// see is not found.
const BRANCHES_OF_RUNS: &str = "branches b JOIN runs r ON r.id = b.run_id AND r.org_id = b.org_id";
/// True when the branch `$1` is one of a run the scope can see.
const VISIBLE_BRANCH: &str =
    "SELECT 1 FROM branches b JOIN runs r ON r.id = b.run_id AND r.org_id = b.org_id WHERE b.id = $1";

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub struct Store {
    /// Request paths' connections (in Task 5 on: under row-level security).
    app: Pool,
    /// Background jobs that span orgs (`crate::jobs`).
    system: Pool,
    /// Each transaction sets its role (`shaked_app`, `shaked_system`): true for a local login,
    /// which is a superuser or a member of both roles.
    set_role: bool,
    tx: broadcast::Sender<Event>,
    // Per-branch locks make `max(seq)+1` appends atomic within this process.
    locks: Mutex<HashMap<String, std::sync::Arc<AsyncMutex<()>>>>,
}

impl Store {
    /// Opens (creating if needed) the metadata database [`DEFAULT_DB`], or the old `arena` one
    /// while there is no `shakedown` ([`pick_db`]).
    ///
    /// `SHAKEDOWN_DB` names another database. Under `SHAKEDOWN_TEST_GUARD` (set by tests that start
    /// shaked) the shared default databases are refused unless `SHAKEDOWN_ALLOW_DEFAULT_DB` is set,
    /// so a test never migrates someone's records ([`db_setting`]).
    pub async fn open(admin: &Admin) -> Result<Store> {
        let set = db_setting(|k| parts::settings::var(k).ok().filter(|v| !v.is_empty()))?;
        let db = match set {
            Some(name) => name,
            None => pick_db(admin, DEFAULT_DB, OLD_DEFAULT_DB).await?,
        };
        if db == OLD_DEFAULT_DB {
            eprintln!("shaked: records in the database arena (its name until 2026-11-19); see README.md, Old names");
        }
        Store::open_named(admin, &db).await
    }

    /// Like `open`, with a custom database name (tests use a fresh one each): makes it when
    /// missing, brings its schema up to date and seeds the local owner ([`prepare_db`]), and marks
    /// what was running when shaked last stopped as interrupted.
    pub async fn open_named(admin: &Admin, name: &str) -> Result<Store> {
        if !admin.exists(name).await? {
            // Concurrent creators may race; tolerate losing.
            if let Err(e) = admin.create_db_from(name, "template1").await {
                if !admin.exists(name).await? {
                    return Err(e).context("creating metadata database");
                }
            }
        }
        prepare_db(admin, name).await?;
        let url = admin.db_url(name);
        let store = Store::with_pools(open::pool(&url)?, open::pool(&url)?, true);
        crate::jobs::interrupt_at_start(&store).await?;
        Ok(store)
    }

    /// Opens a hosted shaked's database (`DbUrls`, every one needed): migrates stream shakedown
    /// once stream identity is at 2, never seeds the local owner, and runs requests and system
    /// jobs on their own logins (members of `shaked_app` and `shaked_system`).
    pub async fn open_hosted(urls: &DbUrls) -> Result<Store> {
        let migrate =
            (urls.migrate.as_deref()).context("hosted mode needs SHAKEDOWN_MIGRATE_PG_URL")?;
        let system =
            (urls.system.as_deref()).context("hosted mode needs SHAKEDOWN_SYSTEM_PG_URL")?;
        anyhow::ensure!(!urls.app.is_empty(), "hosted mode needs SHAKEDOWN_PG_URL");
        open::migrate_hosted(migrate).await?;
        let store = Store::with_pools(open::pool(&urls.app)?, open::pool(system)?, false);
        crate::jobs::interrupt_at_start(&store).await?;
        Ok(store)
    }

    fn with_pools(app: Pool, system: Pool, set_role: bool) -> Store {
        let (tx, _) = broadcast::channel(1024);
        Store {
            app,
            system,
            set_role,
            tx,
            locks: Mutex::new(HashMap::new()),
        }
    }

    fn lock_for(&self, branch: &str) -> std::sync::Arc<AsyncMutex<()>> {
        self.locks
            .lock()
            .unwrap()
            .entry(branch.to_string())
            .or_default()
            .clone()
    }

    /// Every event that can name a save (`run_start`, `tool_call`, `check`), of every branch in every org (a system job),
    /// by branch and seq.
    pub async fn events_naming_saves(&self, s: &SystemScope) -> Result<Vec<Event>> {
        let kinds =
            [EventKind::RunStart, EventKind::ToolCall, EventKind::Check].map(|k| k.as_str());
        let sql =
            format!("SELECT {EVENT_COLS} FROM events WHERE kind = ANY($1) ORDER BY branch_id, seq");
        let tx = self.system_tx(s).await?;
        let rows = tx.query(&sql, &[&kinds.as_slice()]).await?;
        tx.commit().await?;
        rows.iter().map(event_from).collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// The record methods, as `scope`: the only way to them.
    pub fn at<'a>(&'a self, scope: &'a Scope) -> Scoped<'a> {
        Scoped { store: self, scope }
    }
}

/// The store as one scope: every record method runs in a transaction under it, and every query
/// names its org.
#[derive(Clone, Copy)]
pub struct Scoped<'a> {
    store: &'a Store,
    scope: &'a Scope,
}

type Params<'p> = &'p [&'p (dyn ToSql + Sync)];

impl Scoped<'_> {
    /// The org every row this scope writes belongs to.
    fn org(&self) -> uuid::Uuid {
        self.scope.org()
    }

    /// Who writes (`created_by`), when a person.
    fn user(&self) -> Option<uuid::Uuid> {
        self.scope.user()
    }

    async fn query(&self, sql: &str, p: Params<'_>) -> Result<Vec<Row>> {
        let tx = self.store.tx(self.scope).await?;
        let rows = tx.query(sql, p).await?;
        tx.commit().await?;
        Ok(rows)
    }

    async fn query_opt(&self, sql: &str, p: Params<'_>) -> Result<Option<Row>> {
        let tx = self.store.tx(self.scope).await?;
        let row = tx.query_opt(sql, p).await?;
        tx.commit().await?;
        Ok(row)
    }

    async fn query_one(&self, sql: &str, p: Params<'_>) -> Result<Row> {
        let tx = self.store.tx(self.scope).await?;
        let row = tx.query_one(sql, p).await?;
        tx.commit().await?;
        Ok(row)
    }

    async fn execute(&self, sql: &str, p: Params<'_>) -> Result<u64> {
        let tx = self.store.tx(self.scope).await?;
        let n = tx.execute(sql, p).await?;
        tx.commit().await?;
        Ok(n)
    }

    pub async fn create_run(&self, task_id: &str, team: &Team) -> Result<RunId> {
        let id = new_id();
        self.execute(
            "INSERT INTO runs (id, task_id, team, org_id, created_by) VALUES ($1, $2, $3, $4, $5)",
            &[
                &id,
                &task_id,
                &serde_json::to_value(team)?,
                &self.org(),
                &self.user(),
            ],
        )
        .await?;
        Ok(id)
    }

    /// Sets a run's display name without changing its task or recorded events.
    pub async fn name_run(&self, id: &str, name: &str) -> Result<bool> {
        Ok(self
            .execute(
                "UPDATE runs SET name = $2 WHERE id = $1 AND org_id = $3",
                &[&id, &name, &self.org()],
            )
            .await?
            == 1)
    }

    /// `team` is the branch's effective team (the run's team with `changes` applied).
    pub async fn create_branch(
        &self,
        run_id: &str,
        parent: Option<(&str, i64)>,
        team: &Team,
        changes: &Value,
        fork_note: Option<String>,
    ) -> Result<BranchId> {
        let id = new_id();
        let (parent_id, start_seq) = match parent {
            Some((p, s)) => (Some(p.to_string()), Some(s)),
            None => (None, None),
        };
        self.execute(
                "INSERT INTO branches (id, run_id, parent_id, start_seq, team, changes, fork_note, org_id, created_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &id,
                    &run_id,
                    &parent_id,
                    &start_seq,
                    &serde_json::to_value(team)?,
                    changes,
                    &fork_note,
                    &self.org(),
                    &self.user(),
                ],
            )
            .await?;
        Ok(id)
    }

    /// Replaces a branch's team (an external system registered another agent).
    pub async fn set_team(&self, branch_id: &str, team: &Team) -> Result<()> {
        self.execute(
            "UPDATE branches SET team = $2 WHERE id = $1 AND org_id = $3",
            &[&branch_id, &serde_json::to_value(team)?, &self.org()],
        )
        .await?;
        Ok(())
    }

    /// Records how a fork's world was restored: the mode asked for and each part's restore.
    pub async fn set_restore(
        &self,
        branch_id: &str,
        mode: Mode,
        restored: &[RestoredPart],
    ) -> Result<()> {
        self.execute(
            "UPDATE branches SET restore_mode = $2, restored = $3 WHERE id = $1 AND org_id = $4",
            &[
                &branch_id,
                &serde_json::to_value(mode)?.as_str(),
                &serde_json::to_value(restored)?,
                &self.org(),
            ],
        )
        .await?;
        Ok(())
    }

    /// Records `payload` as an event of its kind on a branch, with `overhead` (see `append_with`).
    pub async fn record<P: crate::model::EventPayload>(
        &self,
        branch_id: &str,
        agent: Option<&str>,
        payload: &P,
        overhead: Option<Overhead>,
    ) -> Result<Event> {
        let payload = serde_json::to_value(payload)?;
        self.append_with(branch_id, agent, P::KIND, payload, overhead)
            .await
    }

    /// Records an event of a branch.
    pub async fn append(
        &self,
        branch_id: &str,
        agent: Option<&str>,
        kind: EventKind,
        payload: Value,
    ) -> Result<Event> {
        self.append_with(branch_id, agent, kind, payload, None)
            .await
    }

    /// Records an event of a branch; an object payload also gets `overhead`, the Shakedown's time
    /// since the branch's last event, as `overhead_ms`.
    pub async fn append_with(
        &self,
        branch_id: &str,
        agent: Option<&str>,
        kind: EventKind,
        mut payload: Value,
        overhead: Option<Overhead>,
    ) -> Result<Event> {
        if let (Some(o), Some(obj)) = (overhead, payload.as_object_mut()) {
            obj.insert("overhead_ms".into(), serde_json::to_value(o)?);
        }
        self.insert_event(branch_id, agent, kind, payload).await
    }

    async fn insert_event(
        &self,
        branch_id: &str,
        agent: Option<&str>,
        kind: EventKind,
        payload: Value,
    ) -> Result<Event> {
        let lock = self.store.lock_for(branch_id);
        let _g = lock.lock().await;
        let row = self.query_one(
                &format!(
                    "INSERT INTO events (branch_id, seq, agent, kind, payload, org_id)
                     SELECT $1, coalesce(max(seq), 0) + 1, $2, $3, $4, $5 FROM events WHERE branch_id = $1
                     RETURNING {EVENT_COLS}"
                ),
                &[&branch_id, &agent, &kind.as_str(), &payload, &self.org()],
            )
            .await?;
        let ev = event_from(&row)?;
        let _ = self.store.tx.send(ev.clone()); // no subscribers is fine
        Ok(ev)
    }

    pub async fn events(&self, branch_id: &str) -> Result<Vec<Event>> {
        let rows = self
            .query(
                &format!(
                    "SELECT {EVENT_COLS} FROM events WHERE branch_id = $1 AND org_id = $2
                        AND EXISTS ({VISIBLE_BRANCH}) ORDER BY seq"
                ),
                &[&branch_id, &self.org()],
            )
            .await?;
        rows.iter().map(event_from).collect()
    }

    pub async fn set_status(
        &self,
        branch_id: &str,
        status: BranchStatus,
        error: Option<String>,
    ) -> Result<()> {
        self.execute(
                "UPDATE branches SET status = $2, error = $3,
                 ended_at = CASE WHEN $2 = 'running' THEN NULL ELSE now() END WHERE id = $1 AND org_id = $4",
                &[&branch_id, &status.as_str(), &error, &self.org()],
            )
            .await?;
        Ok(())
    }

    pub async fn branch(&self, id: &str) -> Result<Branch> {
        self.find_branch(id)
            .await?
            .ok_or_else(|| anyhow!("no such branch: {id}"))
    }

    /// Like `branch`, but a missing branch is `Ok(None)` rather than an error.
    pub async fn find_branch(&self, id: &str) -> Result<Option<Branch>> {
        let row = self.query_opt(
                &format!("SELECT {BRANCH_COLS_B} FROM {BRANCHES_OF_RUNS} WHERE b.id = $1 AND b.org_id = $2"),
                &[&id, &self.org()],
            )
            .await?;
        row.as_ref().map(branch_from).transpose()
    }

    pub async fn runs(&self) -> Result<Vec<RunSummary>> {
        let runs = self.query(
                "SELECT id, name, task_id, created_at FROM runs WHERE org_id = $1 ORDER BY created_at DESC, id",
                &[&self.org()],
            )
            .await?;
        let mut by_run = self.branches_by_run().await?;
        Ok(runs
            .iter()
            .map(|r| {
                let id: String = r.get("id");
                RunSummary {
                    branches: by_run.remove(&id).unwrap_or_default(),
                    id,
                    name: r.get("name"),
                    task_id: r.get("task_id"),
                    created_at: r.get("created_at"),
                }
            })
            .collect())
    }

    /// The run `id` with its branches, oldest first; None when there is no such run.
    pub async fn run(&self, id: &str) -> Result<Option<RunSummary>> {
        let sql = "SELECT id, name, task_id, created_at FROM runs WHERE id = $1 AND org_id = $2";
        let Some(r) = self.query_opt(sql, &[&id, &self.org()]).await? else {
            return Ok(None);
        };
        let sql =
            format!("SELECT {BRANCH_COLS} FROM branches WHERE run_id = $1 AND org_id = $2 ORDER BY created_at, id");
        let rows = self.query(&sql, &[&id, &self.org()]).await?;
        Ok(Some(RunSummary {
            branches: rows.iter().map(branch_from).collect::<Result<_>>()?,
            id: r.get("id"),
            name: r.get("name"),
            task_id: r.get("task_id"),
            created_at: r.get("created_at"),
        }))
    }

    /// Every branch, grouped by its run, oldest first.
    async fn branches_by_run(&self) -> Result<HashMap<String, Vec<Branch>>> {
        let rows = self.query(
                &format!(
                    "SELECT {BRANCH_COLS_B} FROM {BRANCHES_OF_RUNS} WHERE b.org_id = $1 ORDER BY b.created_at, b.id"
                ),
                &[&self.org()],
            )
            .await?;
        let mut by_run: HashMap<String, Vec<Branch>> = HashMap::new();
        for r in &rows {
            let b = branch_from(r)?;
            by_run.entry(b.run_id.clone()).or_default().push(b);
        }
        Ok(by_run)
    }

    /// The ids of the run's branches.
    pub async fn run_branches(&self, run_id: &str) -> Result<Vec<BranchId>> {
        let sql = format!(
            "SELECT b.id FROM {BRANCHES_OF_RUNS} WHERE b.run_id = $1 AND b.org_id = $2 ORDER BY b.created_at, b.id"
        );
        let rows = self.query(&sql, &[&run_id, &self.org()]).await?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Deletes the run's branches, events and the run itself.
    pub async fn delete_run(&self, run_id: &str) -> Result<()> {
        let ids = self.run_branches(run_id).await?;
        // One statement, so the branch/parent foreign keys are checked at its end.
        self.execute(
            "WITH ev AS (DELETE FROM events WHERE branch_id = ANY($2) AND org_id = $3),
                      br AS (DELETE FROM branches WHERE run_id = $1 AND org_id = $3)
                 DELETE FROM runs WHERE id = $1 AND org_id = $3",
            &[&run_id, &ids, &self.org()],
        )
        .await?;
        Ok(())
    }

    /// True if some branch of the run has status `running`.
    pub async fn run_has_running(&self, run_id: &str) -> Result<bool> {
        Ok(self.query_one(
                "SELECT EXISTS (SELECT 1 FROM branches WHERE run_id = $1 AND org_id = $2 AND status = 'running')",
                &[&run_id, &self.org()],
            )
            .await?
            .get(0))
    }

    pub async fn run_exists(&self, run_id: &str) -> Result<bool> {
        Ok(self
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM runs WHERE id = $1 AND org_id = $2)",
                &[&run_id, &self.org()],
            )
            .await?
            .get(0))
    }

    /// Copies `from`'s events with `seq < before_seq` onto `to`, keeping their seqs.
    pub async fn copy_prefix(
        &self,
        from_branch: &str,
        to_branch: &str,
        before_seq: i64,
    ) -> Result<()> {
        let lock = self.store.lock_for(to_branch);
        let _g = lock.lock().await;
        self.execute(
            "INSERT INTO events (branch_id, seq, at, agent, kind, payload, org_id)
                 SELECT $2, seq, at, agent, kind, payload, $4 FROM events
                 WHERE branch_id = $1 AND seq < $3 AND org_id = $4",
            &[&from_branch, &to_branch, &before_seq, &self.org()],
        )
        .await?;
        Ok(())
    }
}

fn event_from(r: &Row) -> Result<Event> {
    let kind: String = r.get("kind");
    Ok(Event {
        branch_id: r.get("branch_id"),
        seq: r.get("seq"),
        at: r.get("at"),
        agent: r.get("agent"),
        kind: EventKind::parse(&kind).ok_or_else(|| anyhow!("unknown event kind {kind}"))?,
        payload: r.get("payload"),
    })
}

fn branch_from(r: &Row) -> Result<Branch> {
    let status: String = r.get("status");
    let mut team: Value = r.get("team");
    crate::model::legacy_read_only(&mut team);
    let mode: Option<String> = r.get("restore_mode");
    let restored: Value = r.get("restored");
    Ok(Branch {
        id: r.get("id"),
        run_id: r.get("run_id"),
        parent_id: r.get("parent_id"),
        start_seq: r.get("start_seq"),
        team: serde_json::from_value(team)?,
        changes: r.get("changes"),
        fork_note: r.get("fork_note"),
        status: BranchStatus::parse(&status).ok_or_else(|| anyhow!("unknown status {status}"))?,
        error: r.get("error"),
        created_at: r.get("created_at"),
        ended_at: r.get("ended_at"),
        restore_mode: mode
            .map(|m| serde_json::from_value::<Mode>(Value::String(m)))
            .transpose()?,
        restored: serde_json::from_value(restored)?,
    })
}

#[cfg(test)]
mod tests {
    use super::db_setting;

    fn with<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn the_test_guard_refuses_the_default_databases() {
        assert_eq!(
            db_setting(with(&[])).unwrap(),
            None,
            "a user's shaked uses the default"
        );
        assert_eq!(
            db_setting(with(&[("SHAKEDOWN_DB", "mine")]))
                .unwrap()
                .as_deref(),
            Some("mine")
        );
        let guard = ("SHAKEDOWN_TEST_GUARD", "1");
        assert!(db_setting(with(&[guard])).is_err());
        assert!(db_setting(with(&[guard, ("SHAKEDOWN_DB", "arena")])).is_err());
        assert!(db_setting(with(&[guard, ("SHAKEDOWN_DB", "shakedown")])).is_err());
        let scratch = db_setting(with(&[guard, ("SHAKEDOWN_DB", "shakedown_test_x")]));
        assert_eq!(scratch.unwrap().as_deref(), Some("shakedown_test_x"));
        let allowed = [guard, ("SHAKEDOWN_ALLOW_DEFAULT_DB", "1")];
        assert_eq!(db_setting(with(&allowed)).unwrap(), None);
    }
}
