use std::{collections::HashMap, sync::Mutex};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use tokio::sync::{broadcast, Mutex as AsyncMutex};
use tokio_postgres::{Client, NoTls, Row};

use crate::{model::*, overhead::Overhead, pg::Admin};
use parts::proto::Mode;

mod connections;
mod experiments;
mod reviews;
mod suites;
pub use connections::{is_secret, Connection, NewConnection, REDACTED};
pub use experiments::{NewExperiment, NewSetup};
pub use reviews::{ReviewDone, StoredReview};
pub use suites::NewSuite;

pub type RunId = String;
pub type BranchId = String;

/// The database shaked keeps its records in.
pub const DEFAULT_DB: &str = "shakedown";
/// Its name until 2026-11-19, still used while the server has no [`DEFAULT_DB`].
pub const OLD_DEFAULT_DB: &str = "arena";

/// The database to keep the records in: `name` when it exists, else `old` when that exists (an
/// install from before the rename keeps its records), else `name` (to be made).
pub async fn pick_db(admin: &Admin, name: &str, old: &str) -> Result<String> {
    if !admin.exists(name).await? && admin.exists(old).await? {
        return Ok(old.to_string());
    }
    Ok(name.to_string())
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS runs (
    id text PRIMARY KEY,
    task_id text NOT NULL,
    team jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS branches (
    id text PRIMARY KEY,
    run_id text NOT NULL REFERENCES runs(id),
    parent_id text REFERENCES branches(id),
    start_seq bigint,
    team jsonb NOT NULL,
    changes jsonb NOT NULL DEFAULT '[]',
    fork_note text,
    status text NOT NULL DEFAULT 'running',
    error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    ended_at timestamptz
);
CREATE TABLE IF NOT EXISTS events (
    branch_id text NOT NULL REFERENCES branches(id),
    seq bigint NOT NULL,
    at timestamptz NOT NULL DEFAULT clock_timestamp(),
    agent text,
    kind text NOT NULL,
    payload jsonb NOT NULL,
    PRIMARY KEY (branch_id, seq)
);
ALTER TABLE runs ADD COLUMN IF NOT EXISTS name text;
ALTER TABLE branches ADD COLUMN IF NOT EXISTS restore_mode text;
ALTER TABLE branches ADD COLUMN IF NOT EXISTS restored jsonb NOT NULL DEFAULT '[]';
"#;

const BRANCH_COLS: &str = "id, run_id, parent_id, start_seq, team, changes, fork_note, status, \
     error, created_at, ended_at, restore_mode, restored";
const EVENT_COLS: &str = "branch_id, seq, at, agent, kind, payload";

/// Makes the tables (adding what newer shaked versions keep), and marks what was running when shaked
/// last stopped as interrupted.
async fn migrate(client: &Client) -> Result<()> {
    client.batch_execute(SCHEMA).await?;
    client.batch_execute(suites::SCHEMA).await?;
    client.batch_execute(experiments::SCHEMA).await?;
    client.batch_execute(reviews::SCHEMA).await?;
    client.batch_execute(connections::SCHEMA).await?;
    client
        .execute(
            "UPDATE branches SET status = 'interrupted', ended_at = now() WHERE status = 'running'",
            &[],
        )
        .await?;
    client.execute(suites::INTERRUPT, &[]).await?;
    client.execute(experiments::INTERRUPT, &[]).await?;
    client.execute(reviews::INTERRUPT, &[]).await?;
    Ok(())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub struct Store {
    client: Client,
    tx: broadcast::Sender<Event>,
    // Per-branch locks make `max(seq)+1` appends atomic within this process.
    locks: Mutex<HashMap<String, std::sync::Arc<AsyncMutex<()>>>>,
}

impl Store {
    /// Opens (creating if needed) the metadata database [`DEFAULT_DB`], or the old `arena` one
    /// while there is no `shakedown` ([`pick_db`]).
    pub async fn open(admin: &Admin) -> Result<Store> {
        let db = pick_db(admin, DEFAULT_DB, OLD_DEFAULT_DB).await?;
        if db == OLD_DEFAULT_DB {
            eprintln!("shaked: records in the database arena (its name until 2026-11-19); see README.md, Old names");
        }
        Store::open_named(admin, &db).await
    }

    /// Like `open`, with a custom database name (tests use a fresh one each).
    pub async fn open_named(admin: &Admin, name: &str) -> Result<Store> {
        if !admin.exists(name).await? {
            // Concurrent creators may race; tolerate losing.
            if let Err(e) = admin.create_db_from(name, "template1").await {
                if !admin.exists(name).await? {
                    return Err(e).context("creating metadata database");
                }
            }
        }
        let (client, conn) = tokio_postgres::connect(&admin.db_url(name), NoTls)
            .await
            .with_context(|| format!("connecting to {name}"))?;
        tokio::spawn(conn);
        migrate(&client).await?;
        let (tx, _) = broadcast::channel(1024);
        Ok(Store {
            client,
            tx,
            locks: Mutex::new(HashMap::new()),
        })
    }

    fn lock_for(&self, branch: &str) -> std::sync::Arc<AsyncMutex<()>> {
        self.locks
            .lock()
            .unwrap()
            .entry(branch.to_string())
            .or_default()
            .clone()
    }

    pub async fn create_run(&self, task_id: &str, team: &Team) -> Result<RunId> {
        let id = new_id();
        self.client
            .execute(
                "INSERT INTO runs (id, task_id, team) VALUES ($1, $2, $3)",
                &[&id, &task_id, &serde_json::to_value(team)?],
            )
            .await?;
        Ok(id)
    }

    /// Sets a run's display name without changing its task or recorded events.
    pub async fn name_run(&self, id: &str, name: &str) -> Result<bool> {
        Ok(self
            .client
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
        self.client
            .execute(
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
        self.client
            .execute(
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
        mode: Mode,
        restored: &[RestoredPart],
    ) -> Result<()> {
        self.client
            .execute(
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

    /// Records an event of a branch; an object payload also gets `overhead`, the arena's time
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
        let lock = self.lock_for(branch_id);
        let _g = lock.lock().await;
        let row = self
            .client
            .query_one(
                &format!(
                    "INSERT INTO events (branch_id, seq, agent, kind, payload)
                     SELECT $1, coalesce(max(seq), 0) + 1, $2, $3, $4 FROM events WHERE branch_id = $1
                     RETURNING {EVENT_COLS}"
                ),
                &[&branch_id, &agent, &kind.as_str(), &payload],
            )
            .await?;
        let ev = event_from(&row)?;
        let _ = self.tx.send(ev.clone()); // no subscribers is fine
        Ok(ev)
    }

    pub async fn events(&self, branch_id: &str) -> Result<Vec<Event>> {
        let rows = self
            .client
            .query(
                &format!("SELECT {EVENT_COLS} FROM events WHERE branch_id = $1 ORDER BY seq"),
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
        self.client
            .execute(
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
            .client
            .query_opt(
                &format!("SELECT {BRANCH_COLS} FROM branches WHERE id = $1"),
                &[&id],
            )
            .await?;
        row.as_ref().map(branch_from).transpose()
    }

    pub async fn runs(&self) -> Result<Vec<RunSummary>> {
        let runs = self
            .client
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
        let Some(r) = self.client.query_opt(sql, &[&id]).await? else {
            return Ok(None);
        };
        let sql =
            format!("SELECT {BRANCH_COLS} FROM branches WHERE run_id = $1 ORDER BY created_at, id");
        let rows = self.client.query(&sql, &[&id]).await?;
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
            .client
            .query(
                &format!("SELECT {BRANCH_COLS} FROM branches ORDER BY created_at, id"),
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
        let sql = "SELECT id FROM branches WHERE run_id = $1 ORDER BY created_at, id";
        let rows = self.client.query(sql, &[&run_id]).await?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Every event that can name a save (`run_start`, `tool_call`, `check`), of every branch,
    /// by branch and seq.
    pub async fn events_naming_saves(&self) -> Result<Vec<Event>> {
        let kinds =
            [EventKind::RunStart, EventKind::ToolCall, EventKind::Check].map(|k| k.as_str());
        let sql =
            format!("SELECT {EVENT_COLS} FROM events WHERE kind = ANY($1) ORDER BY branch_id, seq");
        let rows = self.client.query(&sql, &[&kinds.as_slice()]).await?;
        rows.iter().map(event_from).collect()
    }

    /// Deletes the run's branches, events and the run itself.
    pub async fn delete_run(&self, run_id: &str) -> Result<()> {
        let ids = self.run_branches(run_id).await?;
        // One statement, so the branch/parent foreign keys are checked at its end.
        self.client
            .execute(
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
            .client
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM branches WHERE run_id = $1 AND status = 'running')",
                &[&run_id],
            )
            .await?
            .get(0))
    }

    pub async fn run_exists(&self, run_id: &str) -> Result<bool> {
        Ok(self
            .client
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM runs WHERE id = $1)",
                &[&run_id],
            )
            .await?
            .get(0))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// Copies `from`'s events with `seq < before_seq` onto `to`, keeping their seqs.
    pub async fn copy_prefix(
        &self,
        from_branch: &str,
        to_branch: &str,
        before_seq: i64,
    ) -> Result<()> {
        let lock = self.lock_for(to_branch);
        let _g = lock.lock().await;
        self.client
            .execute(
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
            .map(|m| serde_json::from_value::<Mode>(Value::String(m)))
            .transpose()?,
        restored: serde_json::from_value(restored)?,
    })
}
