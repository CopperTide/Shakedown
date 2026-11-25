//! What was exported where: one row per (target, branch), queued when the branch is made (for a
//! target that exports every branch) or when someone asks, sent once the branch has ended and
//! retried with backoff ([`backoff`]). The exporter (`crate::otel::exporter`) works the queue as a
//! system job: each row pairs a branch with a target of the same owner, because it was made by
//! that owner's request. What it sends is read with [`RunRecord`].

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::{
    events::{event_from, EVENT_COLS},
    observability::{open_row, OpenTarget, COLS as TARGET_COLS},
    runs::{branch_from, BRANCH_COLS},
    Scoped, Store, Tx, BRANCHES_OF_RUNS,
};
use crate::{auth::SystemScope, model::*, vault::Vault};

/// How many times an export is tried before it is `failed`.
pub const MAX_ATTEMPTS: i32 = 6;

/// How long to wait after the `attempts`-th failed try: 30 s, 2, 10 and 30 min, then 2 h.
pub fn backoff(attempts: i32) -> chrono::Duration {
    let secs = match attempts {
        ..=1 => 30,
        2 => 120,
        3 => 600,
        4 => 1800,
        _ => 7200,
    };
    chrono::Duration::seconds(secs)
}

/// One export as lists show it.
#[derive(Debug, Clone, Serialize)]
pub struct ExportRow {
    pub target_id: Uuid,
    pub target: String,
    pub branch_id: String,
    pub state: String,
    pub attempts: i32,
    pub spans: Option<i32>,
    pub last_error: String,
    pub exported_at: Option<DateTime<Utc>>,
    pub next_at: DateTime<Utc>,
}

/// An export the exporter may send now: its branch has ended and its time has come.
#[derive(Debug, Clone, PartialEq)]
pub struct Due {
    pub target_id: Uuid,
    pub branch_id: String,
    pub attempts: i32,
}

/// How one try of an export went.
#[derive(Debug, Clone, PartialEq)]
pub enum Tried {
    /// Sent: this many spans.
    Sent(i32),
    /// Not sent, saying why (no header values in it).
    Failed(String),
}

/// A branch with what its run is of: what an export sends.
#[derive(Debug, Clone)]
pub struct RunRecord {
    pub run_id: String,
    pub run_name: Option<String>,
    pub task_id: String,
    pub run_created_at: DateTime<Utc>,
    /// The suite and experiment the run belongs to, `(id, name)`.
    pub suite: Option<(String, String)>,
    pub experiment: Option<(String, String)>,
    pub branch: Branch,
    pub events: Vec<Event>,
}

const EXPORT_COLS: &str = "e.target_id, t.name, e.branch_id, e.state, e.attempts, e.spans, \
     e.last_error, e.exported_at, e.next_at";
const DUE: &str = "SELECT e.target_id, e.branch_id, e.attempts FROM observability_exports e
     JOIN branches b ON b.id = e.branch_id
     WHERE e.state = 'pending' AND e.next_at <= now() AND b.status NOT IN ('running', 'queued')
     ORDER BY e.next_at LIMIT $1";
/// Asked for: from the start, and not due for a minute, so the exporter does not send it while
/// the request that asked does.
const ASK: &str = "INSERT INTO observability_exports (target_id, branch_id, next_at)
     SELECT t.id, $1, now() + interval '1 minute' FROM observability_targets t
     WHERE ($2::uuid IS NULL OR t.id = $2)
     ON CONFLICT (target_id, branch_id) DO UPDATE SET state = 'pending', attempts = 0,
       next_at = excluded.next_at, last_error = ''";
const SENT: &str = "UPDATE observability_exports SET state = 'done', attempts = attempts + 1,
     spans = $3, last_error = '', exported_at = now() WHERE target_id = $1 AND branch_id = $2";
const FAILED: &str =
    "UPDATE observability_exports SET attempts = $3::int, last_error = left($4, 500),
     state = CASE WHEN $3::int >= $5::int THEN 'failed' ELSE 'pending' END, next_at = $6
     WHERE target_id = $1 AND branch_id = $2";
const RUN_OF: &str = "SELECT r.name, r.task_id, r.created_at, s.id, s.name, x.id, x.name
     FROM runs r LEFT JOIN suite_runs sr ON sr.run_id = r.id
     LEFT JOIN suites s ON s.id = sr.suite_id LEFT JOIN experiments x ON x.id = s.experiment_id
     WHERE r.id = $1 LIMIT 1";

fn export_row(r: &tokio_postgres::Row) -> ExportRow {
    ExportRow {
        target_id: r.get(0),
        target: r.get(1),
        branch_id: r.get(2),
        state: r.get(3),
        attempts: r.get(4),
        spans: r.get(5),
        last_error: r.get(6),
        exported_at: r.get(7),
        next_at: r.get(8),
    }
}

/// The pair `(id, name)` of columns `i` and `i + 1`, when the first is set.
fn pair(r: &tokio_postgres::Row, i: usize) -> Option<(String, String)> {
    let id: Option<String> = r.get(i);
    Some((id?, r.get::<_, Option<String>>(i + 1).unwrap_or_default()))
}

/// Branch `id` with its run and events, in `tx` (None: no such branch the transaction sees).
async fn record_in(tx: &Tx, id: &str) -> Result<Option<RunRecord>> {
    let sql = format!("SELECT {BRANCH_COLS} FROM {BRANCHES_OF_RUNS} WHERE b.id = $1");
    let Some(b) = tx.query_opt(&sql, &[&id]).await? else {
        return Ok(None);
    };
    let branch = branch_from(&b)?;
    let r = tx.query_one(RUN_OF, &[&branch.run_id]).await?;
    let sql = format!("SELECT {EVENT_COLS} FROM events WHERE branch_id = $1 ORDER BY seq");
    let rows = tx.query(&sql, &[&id]).await?;
    let events = rows.iter().map(event_from).collect::<Result<_>>()?;
    Ok(Some(RunRecord {
        run_id: branch.run_id.clone(),
        run_name: r.get(0),
        task_id: r.get(1),
        run_created_at: r.get(2),
        suite: pair(&r, 3),
        experiment: pair(&r, 5),
        branch,
        events,
    }))
}

/// Records how a try went, in `tx`.
async fn tried_in(tx: &Tx, due: &Due, tried: &Tried) -> Result<()> {
    let (t, b) = (&due.target_id, &due.branch_id);
    match tried {
        Tried::Sent(spans) => tx.execute(SENT, &[t, b, spans]).await?,
        Tried::Failed(why) => {
            let attempts = due.attempts + 1;
            let next = Utc::now() + backoff(attempts);
            let params: [&(dyn tokio_postgres::types::ToSql + Sync); 6] =
                [t, b, &attempts, why, &MAX_ATTEMPTS, &next];
            tx.execute(FAILED, &params).await?
        }
    };
    Ok(())
}

impl Scoped<'_> {
    /// Branch `id` with its run and events, when the caller sees it.
    pub async fn run_record(&self, id: &str) -> Result<Option<RunRecord>> {
        self.in_tx(async |tx| record_in(tx, id).await).await
    }

    /// Queues branch `id` for target `target` (every target when None) again, from the start.
    pub async fn ask_export(&self, id: &str, target: Option<Uuid>) -> Result<u64> {
        self.execute(ASK, &[&id, &target]).await
    }

    /// Records how a try of `due` went.
    pub async fn record_export(&self, due: &Due, tried: &Tried) -> Result<()> {
        self.in_tx(async |tx| tried_in(tx, due, tried).await).await
    }

    /// The exports of branch `id`, or the latest `limit` of every branch when None.
    pub async fn exports(&self, id: Option<&str>, limit: i64) -> Result<Vec<ExportRow>> {
        let sql = format!(
            "SELECT {EXPORT_COLS} FROM observability_exports e
             JOIN observability_targets t ON t.id = e.target_id
             WHERE ($1::text IS NULL OR e.branch_id = $1)
             ORDER BY e.created_at DESC, e.branch_id LIMIT $2"
        );
        let rows = self.query(&sql, &[&id, &limit]).await?;
        Ok(rows.iter().map(export_row).collect())
    }
}

impl Store {
    /// Up to `limit` exports the exporter may send now (a system job: every owner's).
    pub async fn due_exports(&self, limit: i64) -> Result<Vec<Due>> {
        let tx = self.system_tx(SystemScope::observability_export()).await?;
        let rows = tx.query(DUE, &[&limit]).await?;
        tx.commit().await?;
        let due = rows.iter().map(|r| Due {
            target_id: r.get(0),
            branch_id: r.get(1),
            attempts: r.get(2),
        });
        Ok(due.collect())
    }

    /// The target and the record of `due`, its headers opened (a system job). None when either
    /// is gone.
    pub async fn due_parts(&self, v: &Vault, due: &Due) -> Result<Option<(OpenTarget, RunRecord)>> {
        let tx = self.system_tx(SystemScope::observability_export()).await?;
        let sql = format!(
            "SELECT {TARGET_COLS}, sealed_headers FROM observability_targets WHERE id = $1"
        );
        let target = tx.query_opt(&sql, &[&due.target_id]).await?;
        let record = record_in(&tx, &due.branch_id).await?;
        tx.commit().await?;
        let (Some(t), Some(record)) = (target, record) else {
            return Ok(None);
        };
        Ok(Some((open_row(v, &t)?, record)))
    }

    /// The targets that take OTLP metrics, their headers opened (a system job: every owner's).
    pub async fn metric_targets(&self, v: &Vault) -> Result<Vec<OpenTarget>> {
        let tx = self.system_tx(SystemScope::observability_export()).await?;
        let sql = format!(
            "SELECT {TARGET_COLS}, sealed_headers FROM observability_targets WHERE export_metrics"
        );
        let rows = tx.query(&sql, &[]).await?;
        tx.commit().await?;
        rows.iter().map(|r| open_row(v, r)).collect()
    }

    /// Records how a try of `due` went (a system job).
    pub async fn record_due(&self, due: &Due, tried: &Tried) -> Result<()> {
        let tx = self.system_tx(SystemScope::observability_export()).await?;
        tried_in(&tx, due, tried).await?;
        tx.commit().await
    }
}
