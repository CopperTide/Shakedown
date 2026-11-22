use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use deadpool_postgres::Pool;
use serde_json::Value;
use tokio::sync::{broadcast, Mutex as AsyncMutex};
use tokio_postgres::{types::ToSql, Row};

use crate::{
    auth::{Caller, SystemScope},
    layer::{Layer, LocalLayer},
    model::*,
    overhead::Overhead,
    pg::Admin,
};

mod api_tokens;
mod connections;
mod experiments;
mod open;
mod provider_keys;
mod reviews;
mod run_tokens;
mod saves;
mod suites;
mod tx;
pub use api_tokens::ApiTokenRow;
pub use connections::{is_secret, Connection, NewConnection, REDACTED};
pub use experiments::{NewExperiment, NewSetup};
pub use open::{prepare_db, template_key};
pub use provider_keys::{KeyKind, KeyRow, NameTaken, NewKey, OpenKey};
pub use reviews::{ReviewDone, StoredReview};
pub(crate) use saves::{repoint_saves, saves_on};
pub use saves::{NewSave, SaveRow, SAVE_RETENTION};
pub use suites::NewSuite;
pub use tx::Tx;

pub type RunId = String;
pub type BranchId = String;

/// The database shaked keeps its records in.
pub const DEFAULT_DB: &str = "shakedown";

/// `SHAKEDOWN_DB` (`get` reads a setting), else None (the default database). Under
/// `SHAKEDOWN_TEST_GUARD`, without `SHAKEDOWN_ALLOW_DEFAULT_DB`, a default database is refused,
/// before anything connects.
pub fn db_setting(get: impl Fn(&str) -> Option<String>) -> Result<Option<String>> {
    let named = get("SHAKEDOWN_DB");
    let guarded =
        get("SHAKEDOWN_TEST_GUARD").is_some() && get("SHAKEDOWN_ALLOW_DEFAULT_DB").is_none();
    let default = named.as_deref().is_none_or(|n| n == DEFAULT_DB);
    if guarded && default {
        anyhow::bail!(
            "SHAKEDOWN_TEST_GUARD is set: refusing the shared default database; set SHAKEDOWN_DB to a scratch database (or SHAKEDOWN_ALLOW_DEFAULT_DB=1)"
        );
    }
    Ok(named)
}

const BRANCH_COLS: &str = "id, run_id, parent_id, start_seq, team, changes, fork_note, status, \
     error, created_at, ended_at, restore_mode, restored";
const EVENT_COLS: &str = "branch_id, seq, at, agent, kind, payload";
/// `BRANCH_COLS` of `b` in [`BRANCHES_OF_RUNS`].
const BRANCH_COLS_B: &str =
    "b.id, b.run_id, b.parent_id, b.start_seq, b.team, b.changes, b.fork_note, \
     b.status, b.error, b.created_at, b.ended_at, b.restore_mode, b.restored";
/// Branches joined to their runs: a branch of a run the caller cannot see (under a layer's
/// row-level security) is not found.
const BRANCHES_OF_RUNS: &str = "branches b JOIN runs r ON r.id = b.run_id";
/// True when the branch `$1` is one of a run the caller can see.
const VISIBLE_BRANCH: &str =
    "SELECT 1 FROM branches b JOIN runs r ON r.id = b.run_id WHERE b.id = $1";

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub struct Store {
    /// Request paths' connections.
    pool: Pool,
    /// Background jobs' connections, when they need their own (a hosting layer's); else `pool`.
    system: Option<Pool>,
    /// The deployment's hooks (`crate::layer`): [`LocalLayer`] unless [`Store::with_layer`].
    layer: Arc<dyn Layer>,
    tx: broadcast::Sender<Event>,
    // Per-branch locks make `max(seq)+1` appends atomic within this process.
    locks: Mutex<HashMap<String, std::sync::Arc<AsyncMutex<()>>>>,
}

impl Store {
    /// Opens (creating if needed) the metadata database [`DEFAULT_DB`].
    ///
    /// `SHAKEDOWN_DB` names another database. Under `SHAKEDOWN_TEST_GUARD` (set by tests that start
    /// shaked) the shared default databases are refused unless `SHAKEDOWN_ALLOW_DEFAULT_DB` is set,
    /// so a test never migrates someone's records ([`db_setting`]).
    pub async fn open(admin: &Admin) -> Result<Store> {
        let set = db_setting(|k| parts::settings::var(k).ok().filter(|v| !v.is_empty()))?;
        let db = set.unwrap_or_else(|| DEFAULT_DB.to_string());
        Store::open_named(admin, &db).await
    }

    /// Like `open`, with a custom database name (tests use a fresh one each): makes it when
    /// missing, brings its schema up to date ([`prepare_db`]), and marks
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
        let store = Store::with_pools(open::pool(&url)?, None);
        crate::jobs::interrupt_at_start(&store).await?;
        Ok(store)
    }

    /// A store over pools a hosting binary opened (and migrated), with its layer.
    pub fn from_pools(pool: Pool, system: Option<Pool>, layer: Arc<dyn Layer>) -> Store {
        Store::with_pools(pool, system).with_layer(layer)
    }

    /// This store, calling `layer` at its hooks.
    pub fn with_layer(mut self, layer: Arc<dyn Layer>) -> Store {
        self.layer = layer;
        self
    }

    /// The deployment's hooks.
    pub fn layer(&self) -> &Arc<dyn Layer> {
        &self.layer
    }

    fn with_pools(pool: Pool, system: Option<Pool>) -> Store {
        let (tx, _) = broadcast::channel(1024);
        Store {
            pool,
            system,
            layer: Arc::new(LocalLayer),
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

    /// Every event that can name a save (`run_start`, `tool_call`, `check`), of every branch (a system job),
    /// by branch and seq.
    pub async fn events_naming_saves(&self) -> Result<Vec<Event>> {
        let kinds =
            [EventKind::RunStart, EventKind::ToolCall, EventKind::Check].map(|k| k.as_str());
        let sql =
            format!("SELECT {EVENT_COLS} FROM events WHERE kind = ANY($1) ORDER BY branch_id, seq");
        let tx = self.system_tx(SystemScope::startup_gc()).await?;
        let rows = tx.query(&sql, &[&kinds.as_slice()]).await?;
        tx.commit().await?;
        rows.iter().map(event_from).collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// The record methods, for `caller`: the only way to them.
    pub fn at<'a>(&'a self, caller: &'a Caller) -> Scoped<'a> {
        Scoped {
            store: self,
            caller,
        }
    }
}

/// The store for one caller: every record method runs in a transaction for it.
#[derive(Clone, Copy)]
pub struct Scoped<'a> {
    store: &'a Store,
    caller: &'a Caller,
}

type Params<'p> = &'p [&'p (dyn ToSql + Sync)];

impl Scoped<'_> {
    async fn query(&self, sql: &str, p: Params<'_>) -> Result<Vec<Row>> {
        let tx = self.store.tx(self.caller).await?;
        let rows = tx.query(sql, p).await?;
        tx.commit().await?;
        Ok(rows)
    }

    async fn query_opt(&self, sql: &str, p: Params<'_>) -> Result<Option<Row>> {
        let tx = self.store.tx(self.caller).await?;
        let row = tx.query_opt(sql, p).await?;
        tx.commit().await?;
        Ok(row)
    }

    async fn query_one(&self, sql: &str, p: Params<'_>) -> Result<Row> {
        let tx = self.store.tx(self.caller).await?;
        let row = tx.query_one(sql, p).await?;
        tx.commit().await?;
        Ok(row)
    }

    async fn execute(&self, sql: &str, p: Params<'_>) -> Result<u64> {
        let tx = self.store.tx(self.caller).await?;
        let n = tx.execute(sql, p).await?;
        tx.commit().await?;
        Ok(n)
    }

    pub async fn create_run(&self, task_id: &str, team: &Team) -> Result<RunId> {
        let id = new_id();
        self.execute(
            "INSERT INTO runs (id, task_id, team) VALUES ($1, $2, $3)",
            &[&id, &task_id, &serde_json::to_value(team)?],
        )
        .await?;
        Ok(id)
    }

    /// Sets a run's display name without changing its task or recorded events.
    pub async fn name_run(&self, id: &str, name: &str) -> Result<bool> {
        Ok(self
            .execute("UPDATE runs SET name = $2 WHERE id = $1", &[&id, &name])
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
            "INSERT INTO branches (id, run_id, parent_id, start_seq, team, changes, fork_note)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &id,
                &run_id,
                &parent_id,
                &start_seq,
                &serde_json::to_value(team)?,
                changes,
                &fork_note,
            ],
        )
        .await?;
        Ok(id)
    }

    /// Replaces a branch's team (an external system registered another agent).
    pub async fn set_team(&self, branch_id: &str, team: &Team) -> Result<()> {
        self.execute(
            "UPDATE branches SET team = $2 WHERE id = $1",
            &[&branch_id, &serde_json::to_value(team)?],
        )
        .await?;
        Ok(())
    }

    /// Records how a fork's world was restored: the mode asked for and each part's restore.
    pub async fn set_restore(
        &self,
        branch_id: &str,
        mode: RestoreMode,
        restored: &[RestoredPart],
    ) -> Result<()> {
        self.execute(
            "UPDATE branches SET restore_mode = $2, restored = $3 WHERE id = $1",
            &[
                &branch_id,
                &serde_json::to_value(mode)?.as_str(),
                &serde_json::to_value(restored)?,
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
                    "INSERT INTO events (branch_id, seq, agent, kind, payload)
                     SELECT $1, coalesce(max(seq), 0) + 1, $2, $3, $4 FROM events WHERE branch_id = $1
                     RETURNING {EVENT_COLS}"
                ),
                &[&branch_id, &agent, &kind.as_str(), &payload],
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
                    "SELECT {EVENT_COLS} FROM events WHERE branch_id = $1
                        AND EXISTS ({VISIBLE_BRANCH}) ORDER BY seq"
                ),
                &[&branch_id],
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
                 ended_at = CASE WHEN $2 = 'running' THEN NULL ELSE now() END WHERE id = $1",
            &[&branch_id, &status.as_str(), &error],
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
        let row = self
            .query_opt(
                &format!("SELECT {BRANCH_COLS_B} FROM {BRANCHES_OF_RUNS} WHERE b.id = $1"),
                &[&id],
            )
            .await?;
        row.as_ref().map(branch_from).transpose()
    }

    pub async fn runs(&self) -> Result<Vec<RunSummary>> {
        let runs = self
            .query(
                "SELECT id, name, task_id, created_at FROM runs ORDER BY created_at DESC, id",
                &[],
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
        let sql = "SELECT id, name, task_id, created_at FROM runs WHERE id = $1";
        let Some(r) = self.query_opt(sql, &[&id]).await? else {
            return Ok(None);
        };
        let sql =
            format!("SELECT {BRANCH_COLS} FROM branches WHERE run_id = $1 ORDER BY created_at, id");
        let rows = self.query(&sql, &[&id]).await?;
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
        let rows = self
            .query(
                &format!(
                    "SELECT {BRANCH_COLS_B} FROM {BRANCHES_OF_RUNS} ORDER BY b.created_at, b.id"
                ),
                &[],
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
            "SELECT b.id FROM {BRANCHES_OF_RUNS} WHERE b.run_id = $1 ORDER BY b.created_at, b.id"
        );
        let rows = self.query(&sql, &[&run_id]).await?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Deletes the run's branches, events and the run itself.
    pub async fn delete_run(&self, run_id: &str) -> Result<()> {
        let ids = self.run_branches(run_id).await?;
        // One statement, so the branch/parent foreign keys are checked at its end.
        self.execute(
            "WITH ev AS (DELETE FROM events WHERE branch_id = ANY($2)),
                      br AS (DELETE FROM branches WHERE run_id = $1)
                 DELETE FROM runs WHERE id = $1",
            &[&run_id, &ids],
        )
        .await?;
        Ok(())
    }

    /// True if some branch of the run has status `running`.
    pub async fn run_has_running(&self, run_id: &str) -> Result<bool> {
        Ok(self
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM branches WHERE run_id = $1 AND status = 'running')",
                &[&run_id],
            )
            .await?
            .get(0))
    }

    pub async fn run_exists(&self, run_id: &str) -> Result<bool> {
        Ok(self
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM runs WHERE id = $1)",
                &[&run_id],
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
            "INSERT INTO events (branch_id, seq, at, agent, kind, payload)
                 SELECT $2, seq, at, agent, kind, payload FROM events
                 WHERE branch_id = $1 AND seq < $3",
            &[&from_branch, &to_branch, &before_seq],
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
            .map(|m| serde_json::from_value::<RestoreMode>(Value::String(m)))
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
        assert!(db_setting(with(&[guard, ("SHAKEDOWN_DB", "shakedown")])).is_err());
        let scratch = db_setting(with(&[guard, ("SHAKEDOWN_DB", "shakedown_test_x")]));
        assert_eq!(scratch.unwrap().as_deref(), Some("shakedown_test_x"));
        let allowed = [guard, ("SHAKEDOWN_ALLOW_DEFAULT_DB", "1")];
        assert_eq!(db_setting(with(&allowed)).unwrap(), None);
    }
}
