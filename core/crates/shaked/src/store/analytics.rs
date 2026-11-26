//! The analytics records: each ended branch's figures (`branches.stats`, kept by shaked's
//! analytics job), and the summary aggregated over them in SQL. The summary joins each branch to
//! its run in the caller's transaction, so a caller counts only the runs it may see.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use tokio_postgres::{types::ToSql, Row};

use super::{
    events::{event_from, LEAN_PAYLOAD},
    runs::{branch_from, BRANCH_COLS},
    Scoped, Store, Tx, BRANCHES_OF_RUNS,
};
use crate::{
    analytics::{filter::bucket_for, stats::VERSION, Query},
    auth::SystemScope,
    model::{Branch, Event},
};

/// An ended branch's figures are current: worked out, of this shape, for the end it has now.
/// (`ended_at` is kept as epoch seconds, which read the same in every session's time zone.)
fn current() -> String {
    format!(
        "b.status NOT IN ('running', 'queued') AND b.ended_at IS NOT NULL
     AND b.stats ? 'setup' AND (b.stats->>'v')::int = {VERSION}
     AND b.stats->'ended_at' = to_jsonb(extract(epoch FROM b.ended_at))"
    )
}

/// The runs a summary counts (each run's first branch, with current figures), unpacked; `$1`–`$7`
/// are a [`Query`]'s filters ([`bind`]).
fn facts() -> String {
    format!(
        "SELECT b.created_at AS at, b.stats->>'world' AS world, b.stats->>'scenario' AS scenario,
       b.stats->>'setup' AS setup, b.stats->>'model' AS model,
       (b.stats->>'seconds')::float8 AS seconds, (b.stats->>'cost_usd')::float8 AS cost,
       (b.stats->>'tokens')::bigint AS tokens,
       b.status IN ('passed', 'failed', 'budget_exceeded') AS judged, b.status = 'passed' AS passed
  FROM {BRANCHES_OF_RUNS}
 WHERE b.parent_id IS NULL AND {}
   AND ($1::timestamptz IS NULL OR b.created_at >= $1)
   AND ($2::timestamptz IS NULL OR b.created_at < $2)
   AND (cardinality($3::text[]) = 0 OR b.stats->>'world' = ANY($3))
   AND (cardinality($4::text[]) = 0 OR b.stats->>'scenario' = ANY($4))
   AND (cardinality($5::text[]) = 0 OR b.stats->>'setup' = ANY($5))
   AND (cardinality($6::text[]) = 0 OR b.stats->>'model' = ANY($6))
   AND (cardinality($7::text[]) = 0 OR b.status = ANY($7))",
        current()
    )
}

const TOTALS: &str =
    "SELECT count(*), count(*) FILTER (WHERE judged), count(*) FILTER (WHERE passed),
       coalesce(sum(cost), 0)::float8, count(cost), coalesce(sum(tokens), 0)::bigint,
       percentile_cont(0.5) WITHIN GROUP (ORDER BY seconds) FILTER (WHERE judged),
       min(at), max(at)
  FROM f";
/// `$8` the bucket (`day`, `week`), `$9` the group (`setup`, `model`).
const SERIES: &str = "SELECT date_trunc($8::text, at, 'UTC'),
       CASE $9::text WHEN 'model' THEN model ELSE setup END AS g,
       count(*) FILTER (WHERE judged), count(*) FILTER (WHERE passed)
  FROM f GROUP BY 1, 2 ORDER BY 1, 2";
const POINTS: &str = "SELECT CASE $8::text WHEN 'model' THEN model ELSE setup END AS g,
       count(*), count(*) FILTER (WHERE judged), count(*) FILTER (WHERE passed),
       coalesce(sum(cost), 0)::float8, count(cost), coalesce(sum(tokens), 0)::bigint
  FROM f GROUP BY 1 ORDER BY 2 DESC, 1 LIMIT 50";
const BOARD: &str = "SELECT setup, world, scenario, count(*), count(*) FILTER (WHERE judged),
       count(*) FILTER (WHERE passed), coalesce(sum(cost), 0)::float8, count(cost),
       percentile_cont(0.5) WITHIN GROUP (ORDER BY seconds) FILTER (WHERE judged)
  FROM f GROUP BY 1, 2, 3 ORDER BY 1, 2, 3 LIMIT 500";

/// Every value each filter can take among the runs made in the time range (`$1`, `$2`).
fn options() -> String {
    format!(
        "SELECT DISTINCT x.k, x.v FROM {BRANCHES_OF_RUNS}
 CROSS JOIN LATERAL (VALUES ('world', b.stats->>'world'), ('scenario', b.stats->>'scenario'),
                            ('setup', b.stats->>'setup'), ('model', b.stats->>'model'),
                            ('status', b.status)) x(k, v)
 WHERE b.parent_id IS NULL AND {}
   AND ($1::timestamptz IS NULL OR b.created_at >= $1)
   AND ($2::timestamptz IS NULL OR b.created_at < $2)
   AND x.v <> '' ORDER BY 1, 2 LIMIT 2000",
        current()
    )
}

/// An ended branch whose figures are missing, of another shape, or for an earlier end.
fn stale() -> String {
    format!(
        "b.status NOT IN ('running', 'queued') AND b.ended_at IS NOT NULL
     AND (b.stats IS NULL OR (b.stats->>'v')::int IS DISTINCT FROM {VERSION}
          OR b.stats->'ended_at' IS DISTINCT FROM to_jsonb(extract(epoch FROM b.ended_at)))"
    )
}

/// Runs not in the figures yet: ended but not worked out, and still going.
fn pending() -> String {
    format!(
        "SELECT count(*) FILTER (WHERE {}), count(*) FILTER (WHERE b.status IN ('running', 'queued'))
  FROM {BRANCHES_OF_RUNS} WHERE b.parent_id IS NULL",
        stale()
    )
}

/// The summary's headline figures.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Totals {
    pub runs: i64,
    /// Runs that count for the pass rate: passed, failed or over budget (not stopped, interrupted
    /// or failed by the platform).
    pub judged: i64,
    pub passed: i64,
    /// `passed / judged`; None with nothing judged.
    pub pass_rate: Option<f64>,
    pub cost_usd: f64,
    /// The mean cost of the runs whose cost is known.
    pub cost_per_run: Option<f64>,
    /// Every run's cost over the runs that passed: what one pass costs.
    pub cost_per_pass: Option<f64>,
    /// The median wall-clock time of the judged runs.
    pub median_seconds: Option<f64>,
    pub tokens: i64,
    /// Runs whose cost is not known (left out of the cost figures).
    pub uncosted: i64,
}

/// What is not in the figures yet.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Pending {
    /// Ended runs whose figures shaked is still working out (seconds, normally).
    pub unprocessed: i64,
    /// Runs still going.
    pub running: i64,
}

/// One group's pass rate in one bucket.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeriesPoint {
    pub t: DateTime<Utc>,
    pub group: String,
    pub judged: i64,
    pub passed: i64,
    pub pass_rate: Option<f64>,
}

/// One group (a setup or a model) over the whole range.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupPoint {
    pub group: String,
    pub runs: i64,
    pub judged: i64,
    pub passed: i64,
    pub pass_rate: Option<f64>,
    pub cost_usd: f64,
    pub cost_per_run: Option<f64>,
    pub tokens: i64,
}

/// One setup on one scenario (of one world).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoardRow {
    pub setup: String,
    pub world: String,
    pub scenario: String,
    pub runs: i64,
    pub judged: i64,
    pub passed: i64,
    pub pass_rate: Option<f64>,
    pub cost_per_run: Option<f64>,
    pub median_seconds: Option<f64>,
    pub cost_per_pass: Option<f64>,
}

/// The values each filter can take.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Options {
    pub world: Vec<String>,
    pub scenario: Vec<String>,
    pub setup: Vec<String>,
    pub model: Vec<String>,
    pub status: Vec<String>,
}

/// Everything the Analytics overview shows, for one filter.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    /// What `series` and `points` are per: `setup` or `model`.
    pub group: &'static str,
    /// The series' bucket: `day` or `week` (UTC).
    pub bucket: &'static str,
    pub totals: Totals,
    pub pending: Pending,
    pub series: Vec<SeriesPoint>,
    pub points: Vec<GroupPoint>,
    pub leaderboard: Vec<BoardRow>,
    pub options: Options,
}

/// A branch's record as the analytics job reads it: model-call bodies left out.
pub struct StatsRecord {
    pub branch: Branch,
    pub task_id: String,
    pub events: Vec<Event>,
}

/// `num / den`, None for an empty denominator.
fn ratio(num: f64, den: i64) -> Option<f64> {
    (den > 0).then(|| num / den as f64)
}

fn text(r: &Row, i: usize) -> String {
    r.get::<_, Option<String>>(i).unwrap_or_default()
}

/// The seven filter parameters of [`facts`].
fn bind(q: &Query) -> Vec<&(dyn ToSql + Sync)> {
    vec![
        &q.from,
        &q.to,
        &q.world,
        &q.scenario,
        &q.setup,
        &q.model,
        &q.status,
    ]
}

/// The rows of `sql` over the runs `q` counts, with `more` parameters after the filter's.
async fn over(tx: &Tx, q: &Query, sql: &str, more: &[&(dyn ToSql + Sync)]) -> Result<Vec<Row>> {
    let mut p = bind(q);
    p.extend_from_slice(more);
    tx.query(&format!("WITH f AS ({}) {sql}", facts()), &p)
        .await
}

fn totals(r: &Row) -> Totals {
    let (judged, passed, cost, costed) = (r.get(1), r.get::<_, i64>(2), r.get(3), r.get(4));
    Totals {
        runs: r.get(0),
        judged,
        passed,
        pass_rate: ratio(passed as f64, judged),
        cost_usd: cost,
        cost_per_run: ratio(cost, costed),
        cost_per_pass: ratio(cost, passed),
        median_seconds: r.get(6),
        tokens: r.get(5),
        uncosted: r.get::<_, i64>(0) - costed,
    }
}

fn series_point(r: &Row) -> SeriesPoint {
    let (judged, passed) = (r.get(2), r.get::<_, i64>(3));
    SeriesPoint {
        t: r.get(0),
        group: text(r, 1),
        judged,
        passed,
        pass_rate: ratio(passed as f64, judged),
    }
}

fn group_point(r: &Row) -> GroupPoint {
    let (judged, passed, cost) = (r.get(2), r.get::<_, i64>(3), r.get(4));
    GroupPoint {
        group: text(r, 0),
        runs: r.get(1),
        judged,
        passed,
        pass_rate: ratio(passed as f64, judged),
        cost_usd: cost,
        cost_per_run: ratio(cost, r.get(5)),
        tokens: r.get(6),
    }
}

fn board_row(r: &Row) -> BoardRow {
    let (judged, passed, cost) = (r.get(4), r.get::<_, i64>(5), r.get(6));
    BoardRow {
        setup: text(r, 0),
        world: text(r, 1),
        scenario: text(r, 2),
        runs: r.get(3),
        judged,
        passed,
        pass_rate: ratio(passed as f64, judged),
        cost_per_run: ratio(cost, r.get(7)),
        median_seconds: r.get(8),
        cost_per_pass: ratio(cost, passed),
    }
}

fn options_of(rows: &[Row]) -> Options {
    let mut o = Options::default();
    for r in rows {
        let list = match r.get::<_, &str>(0) {
            "world" => &mut o.world,
            "scenario" => &mut o.scenario,
            "setup" => &mut o.setup,
            "model" => &mut o.model,
            _ => &mut o.status,
        };
        list.push(text(r, 1));
    }
    o
}

/// The series' bucket for `q`: by its range, else by the first and last run it counts.
fn bucket_of(q: &Query, totals: &Row) -> &'static str {
    let first: Option<DateTime<Utc>> = totals.get(7);
    let last: Option<DateTime<Utc>> = totals.get(8);
    match (q.from.or(first), q.to.or(last)) {
        (Some(a), Some(b)) => bucket_for(a, b),
        _ => "day",
    }
}

/// The rows of `sql` over the runs `q` counts, each read with `each`.
async fn read<T>(
    tx: &Tx,
    q: &Query,
    sql: &str,
    more: &[&(dyn ToSql + Sync)],
    each: fn(&Row) -> T,
) -> Result<Vec<T>> {
    Ok(over(tx, q, sql, more).await?.iter().map(each).collect())
}

/// The summary of `q`, in `tx`.
async fn summary_in(tx: &Tx, q: &Query) -> Result<Summary> {
    let rows = over(tx, q, TOTALS, &[]).await?;
    let Some(t) = rows.first() else {
        anyhow::bail!("the totals came back empty");
    };
    let (bucket, group) = (bucket_of(q, t), q.group.as_str());
    let p = tx.query_one(&pending(), &[]).await?;
    Ok(Summary {
        group,
        bucket,
        totals: totals(t),
        pending: Pending {
            unprocessed: p.get(0),
            running: p.get(1),
        },
        series: read(tx, q, SERIES, &[&bucket, &group], series_point).await?,
        points: read(tx, q, POINTS, &[&group], group_point).await?,
        leaderboard: read(tx, q, BOARD, &[], board_row).await?,
        options: options_of(&tx.query(&options(), &[&q.from, &q.to]).await?),
    })
}

impl Scoped<'_> {
    /// The Analytics overview's figures for `q`, over the runs the caller sees, in one transaction.
    pub async fn analytics_summary(&self, q: &Query) -> Result<Summary> {
        self.in_tx(async |tx| summary_in(tx, q).await).await
    }
}

/// Ended branches whose figures are to be worked out, newest end first.
fn unfilled() -> String {
    format!(
        "SELECT b.id FROM branches b WHERE {} ORDER BY b.ended_at DESC LIMIT $1",
        stale()
    )
}

/// Keeps a branch's figures, for the end it was worked out for (`$3`): unless it ended again since.
const PUT: &str = "UPDATE branches
   SET stats = $2::jsonb || jsonb_build_object('ended_at', to_jsonb(extract(epoch FROM ended_at)))
 WHERE id = $1 AND ended_at = $3";

impl Store {
    /// Up to `limit` ended branches whose figures are to be worked out (a system job: every
    /// owner's).
    pub async fn unfilled_stats(&self, limit: i64) -> Result<Vec<String>> {
        let tx = self.system_tx(SystemScope::analytics_stats()).await?;
        let rows = tx.query(&unfilled(), &[&limit]).await?;
        tx.commit().await?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Branch `id` with its run's task and its events without model-call bodies (a system job);
    /// None when it is gone.
    pub async fn stats_record(&self, id: &str) -> Result<Option<StatsRecord>> {
        let tx = self.system_tx(SystemScope::analytics_stats()).await?;
        let sql =
            format!("SELECT {BRANCH_COLS}, r.task_id FROM {BRANCHES_OF_RUNS} WHERE b.id = $1");
        let Some(row) = tx.query_opt(&sql, &[&id]).await? else {
            return Ok(None);
        };
        let sql = format!(
            "SELECT branch_id, seq, at, agent, kind, {LEAN_PAYLOAD} AS payload FROM events
              WHERE branch_id = $1 ORDER BY seq"
        );
        let events = tx.query(&sql, &[&id]).await?;
        tx.commit().await?;
        Ok(Some(StatsRecord {
            branch: branch_from(&row)?,
            task_id: row.get("task_id"),
            events: events.iter().map(event_from).collect::<Result<_>>()?,
        }))
    }

    /// Keeps `stats` for branch `id` if it still ended at `ended_at` (a system job).
    pub async fn put_stats(
        &self,
        id: &str,
        ended_at: DateTime<Utc>,
        stats: &Value,
    ) -> Result<bool> {
        let tx = self.system_tx(SystemScope::analytics_stats()).await?;
        let n = tx.execute(PUT, &[&id, stats, &ended_at]).await?;
        tx.commit().await?;
        Ok(n == 1)
    }

    /// Marks branch `id`'s record as unreadable for its current end, so the job does not try it
    /// again until it ends again; the summary leaves it out (a system job).
    pub async fn mark_stats_unreadable(&self, id: &str, why: &str) -> Result<()> {
        let tx = self.system_tx(SystemScope::analytics_stats()).await?;
        let stats = json!({ "v": VERSION, "unreadable": why });
        let sql = "UPDATE branches
   SET stats = $2::jsonb || jsonb_build_object('ended_at', to_jsonb(extract(epoch FROM ended_at)))
 WHERE id = $1 AND ended_at IS NOT NULL";
        tx.execute(sql, &[&id, &stats]).await?;
        tx.commit().await?;
        Ok(())
    }
}
