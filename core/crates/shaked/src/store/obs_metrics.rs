//! What the metrics (`GET /metrics`, OTLP metrics) count, read from the records: each branch the
//! caller sees with its world and setup, and its model calls summed by model and provider. Only a
//! branch's own calls count (a fork's inherited ones are its parent's). An ended branch's sums
//! never change, so they are read once and kept ([`ENDED`]); a running branch's are read again
//! on each scrape.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use anyhow::Result;
use chrono::{DateTime, Utc};

use super::{Scoped, Store, Tx};
use crate::{auth::SystemScope, model::BranchStatus};

/// A branch as the metrics see it.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchLine {
    pub id: String,
    pub status: BranchStatus,
    pub world: String,
    pub setup: String,
    pub created_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

/// One branch's model calls with one model and provider.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallLine {
    pub model: String,
    pub provider: String,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    /// Calls whose cost is not known.
    pub uncosted: i64,
    /// How long each call took, in seconds (calls recorded with a duration).
    pub seconds: Vec<f64>,
}

/// What the metrics are made from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub branches: Vec<BranchLine>,
    /// Each branch's call lines, by branch id.
    pub calls: HashMap<String, Vec<CallLine>>,
}

/// The call lines of ended branches, by branch id: what never changes again. Keyed by branch
/// ids, which are unique, and read only for branches a caller sees.
static ENDED: OnceLock<Mutex<HashMap<String, Vec<CallLine>>>> = OnceLock::new();

fn ended() -> &'static Mutex<HashMap<String, Vec<CallLine>>> {
    ENDED.get_or_init(Default::default)
}

const BRANCHES: &str = "SELECT b.id, b.status, b.created_at, b.ended_at,
       coalesce(rs.world, ''), coalesce(b.team->>'name', b.team->>'id', '')
  FROM branches b JOIN runs r ON r.id = b.run_id
  LEFT JOIN LATERAL (SELECT e.payload->>'benchmark' AS world FROM events e
                      WHERE e.branch_id = b.id AND e.kind = 'run_start' ORDER BY e.seq LIMIT 1) rs ON true";

/// A number at `path` of the payload, when it is one.
macro_rules! num {
    ($path:literal) => {
        concat!(
            "CASE WHEN jsonb_typeof(e.payload",
            $path,
            ") = 'number' THEN (e.payload",
            $path,
            ")::text::float8 END"
        )
    };
}

const CALLS: &str = concat!(
    "SELECT e.branch_id,
       coalesce(e.payload->'request'->>'model', e.payload->'response'->>'model', 'unknown'),
       coalesce(e.payload->>'provider', 'unknown'), count(*),
       coalesce(sum(",
    num!("->'usage'->'prompt_tokens'"),
    "), 0)::bigint,
       coalesce(sum(",
    num!("->'usage'->'completion_tokens'"),
    "), 0)::bigint,
       coalesce(sum(",
    num!("->'cost_usd'"),
    "), 0)::float8,
       count(*) FILTER (WHERE jsonb_typeof(e.payload->'cost_usd') IS DISTINCT FROM 'number'),
       coalesce(array_agg(",
    num!("->'duration_ms'"),
    ") FILTER (WHERE jsonb_typeof(e.payload->'duration_ms') = 'number'), '{}')
  FROM events e JOIN branches b ON b.id = e.branch_id
 WHERE e.branch_id = ANY($1) AND e.kind = 'model_call' AND e.seq >= coalesce(b.start_seq, 0)
 GROUP BY 1, 2, 3 ORDER BY 1, 2, 3"
);

fn branch_line(r: &tokio_postgres::Row) -> Option<BranchLine> {
    Some(BranchLine {
        id: r.get(0),
        status: BranchStatus::parse(r.get(1))?,
        created_at: r.get(2),
        ended_at: r.get(3),
        world: r.get(4),
        setup: r.get(5),
    })
}

fn call_line(r: &tokio_postgres::Row) -> (String, CallLine) {
    let ms: Vec<f64> = r.get(8);
    let line = CallLine {
        model: r.get(1),
        provider: r.get(2),
        calls: r.get(3),
        input_tokens: r.get(4),
        output_tokens: r.get(5),
        cost_usd: r.get(6),
        uncosted: r.get(7),
        seconds: ms.into_iter().map(|m| m / 1000.0).collect(),
    };
    (r.get(0), line)
}

/// The kept call lines of `branches` that ended, and the ids of the rest (to read).
fn kept_or_unread(branches: &[BranchLine]) -> (HashMap<String, Vec<CallLine>>, Vec<String>) {
    let kept = ended().lock().expect("the metrics cache is never poisoned");
    let mut calls = HashMap::new();
    let mut unread = vec![];
    for b in branches {
        match kept.get(&b.id) {
            Some(c) if !b.status.goes_on() => {
                calls.insert(b.id.clone(), c.clone());
            }
            _ => unread.push(b.id.clone()),
        }
    }
    (calls, unread)
}

/// Keeps the call lines just read of the branches that ended.
fn keep(branches: &[BranchLine], read: &[String], calls: &HashMap<String, Vec<CallLine>>) {
    let mut kept = ended().lock().expect("the metrics cache is never poisoned");
    for b in branches
        .iter()
        .filter(|b| !b.status.goes_on() && read.contains(&b.id))
    {
        kept.insert(b.id.clone(), calls.get(&b.id).cloned().unwrap_or_default());
    }
}

/// The snapshot of the branches `filter` (a `WHERE` over `b`, may be empty) picks, in `tx`.
async fn snapshot_in(
    tx: &Tx,
    filter: &str,
    p: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<Snapshot> {
    let sql = format!("{BRANCHES} {filter} ORDER BY b.created_at, b.id");
    let rows = tx.query(&sql, p).await?;
    let branches: Vec<BranchLine> = rows.iter().filter_map(branch_line).collect();
    let (mut calls, unread) = kept_or_unread(&branches);
    for r in tx.query(CALLS, &[&unread]).await? {
        let (id, line) = call_line(&r);
        calls.entry(id).or_default().push(line);
    }
    keep(&branches, &unread, &calls);
    Ok(Snapshot { branches, calls })
}

impl Scoped<'_> {
    /// Every branch the caller sees, for the metrics.
    pub async fn metrics_snapshot(&self) -> Result<Snapshot> {
        self.in_tx(async |tx| snapshot_in(tx, "", &[]).await).await
    }
}

impl Store {
    /// The branches queued for target `target`, for its OTLP metrics (a system job: the target's
    /// owner's branches, as its export rows pair them).
    pub async fn target_snapshot(&self, target: uuid::Uuid) -> Result<Snapshot> {
        let tx = self.system_tx(SystemScope::observability_export()).await?;
        let filter =
            "WHERE b.id IN (SELECT branch_id FROM observability_exports WHERE target_id = $1)";
        let s = snapshot_in(&tx, filter, &[&target]).await?;
        tx.commit().await?;
        Ok(s)
    }
}
