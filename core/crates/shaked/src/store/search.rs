//! Search over every run's steps, as the caller: the events whose text (`shakedown.step_text`,
//! migration 8) matches, through the `events_search` index, each with a snippet of where it
//! matched. Events join their branch and run, so a caller finds only the runs it may see.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio_postgres::{types::ToSql, Row};

use super::{Scoped, Tx};
use crate::analytics::search::{SearchQuery, HIT_END, HIT_START, MAX_OFFSET};

/// The matching events, `$1`–`$5` a [`SearchQuery`]'s words and filters. The match is written as
/// the index is, or the planner cannot use it.
const MATCHES: &str =
    "FROM events e JOIN branches b ON b.id = e.branch_id JOIN runs r ON r.id = b.run_id
 WHERE to_tsvector('english'::regconfig, shakedown.step_text(e.kind, e.payload))
       @@ websearch_to_tsquery('english', $1)
   AND (cardinality($2::text[]) = 0 OR e.kind = ANY($2))
   AND (cardinality($3::text[]) = 0 OR e.agent = ANY($3))
   AND (cardinality($4::text[]) = 0 OR r.id = ANY($4))
   AND (cardinality($5::text[]) = 0 OR EXISTS (
         SELECT 1 FROM suite_runs sr JOIN suites s ON s.id = sr.suite_id
          WHERE sr.run_id = r.id AND s.experiment_id = ANY($5)))";

/// One page, newest first; the snippet is made for the page's rows only (`$6` its options, `$7`
/// the limit, `$8` the offset).
fn page() -> String {
    format!(
        "SELECT p.run_id, p.run_name, p.branch_id, p.seq, p.kind, p.agent, p.at,
       ts_headline('english', shakedown.step_text(p.kind, p.payload), websearch_to_tsquery('english', $1), $6)
  FROM (SELECT r.id AS run_id, coalesce(r.name, r.task_id) AS run_name, e.branch_id, e.seq, e.kind, e.agent,
               e.at, e.payload
          {MATCHES} ORDER BY e.at DESC, e.branch_id, e.seq LIMIT $7 OFFSET $8) p
 ORDER BY p.at DESC, p.branch_id, p.seq"
    )
}

/// How many match, counted up to `$6`.
fn total() -> String {
    format!("SELECT count(*) FROM (SELECT 1 {MATCHES} LIMIT $6) m")
}

/// Matches per kind and agent among the newest `$6`, whatever kinds and agents are picked
/// (`$2`, `$3` are bound empty), so the filters show what else there is.
fn facets() -> String {
    format!(
        "SELECT kind, agent, count(*) FROM
  (SELECT e.kind, e.agent {MATCHES} ORDER BY e.at DESC LIMIT $6) m GROUP BY 1, 2"
    )
}

/// One step found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit {
    pub run_id: String,
    /// The run's name, else its scenario.
    pub run_name: String,
    pub branch_id: String,
    /// The step's number in its branch (the run page's `?step=`).
    pub seq: i64,
    pub kind: String,
    pub agent: Option<String>,
    pub at: DateTime<Utc>,
    /// Where it matched, each match between U+E000 and U+E001.
    pub snippet: String,
}

/// How many matches there are of one kind or agent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FacetCount {
    pub name: String,
    pub count: i64,
}

/// What a search found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Matches in all, counted up to [`MAX_OFFSET`].
    pub total: i64,
    /// True when there are more than `total`.
    pub total_capped: bool,
    /// The offset of the next page; None on the last.
    pub next_offset: Option<i64>,
    /// Matches per kind and per agent (stepless agents left out), ignoring the kind and agent
    /// filters, most first.
    pub kinds: Vec<FacetCount>,
    pub agents: Vec<FacetCount>,
}

/// `ts_headline`'s options: a couple of short fragments, matches between the markers.
fn headline_options() -> String {
    format!(
        "MaxFragments=2, MaxWords=24, MinWords=10, FragmentDelimiter=\" … \", StartSel={HIT_START}, StopSel={HIT_END}"
    )
}

fn hit(r: &Row) -> SearchHit {
    SearchHit {
        run_id: r.get(0),
        run_name: r.get(1),
        branch_id: r.get(2),
        seq: r.get(3),
        kind: r.get(4),
        agent: r.get(5),
        at: r.get(6),
        snippet: r.get::<_, Option<String>>(7).unwrap_or_default(),
    }
}

/// Facet counts from `(kind, agent, count)` rows, most first, then by name.
fn tally(rows: &[Row], pick: fn(&Row) -> Option<String>) -> Vec<FacetCount> {
    let mut counts = std::collections::BTreeMap::<String, i64>::new();
    for r in rows {
        if let Some(name) = pick(r) {
            *counts.entry(name).or_default() += r.get::<_, i64>(2);
        }
    }
    let mut out: Vec<_> = counts
        .into_iter()
        .map(|(name, count)| FacetCount { name, count })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    out
}

type Param<'a> = &'a (dyn ToSql + Sync);

/// `$1`–`$5` for [`MATCHES`] (the kinds and agents only when `picked`), then `more`.
fn bind<'a>(q: &'a SearchQuery, picked: bool, more: &[Param<'a>]) -> Vec<Param<'a>> {
    const NONE: &Vec<String> = &Vec::new();
    let (kind, agent) = if picked {
        (&q.kind, &q.agent)
    } else {
        (NONE, NONE)
    };
    let mut p: Vec<Param<'a>> = vec![&q.q, kind, agent, &q.run, &q.experiment];
    p.extend_from_slice(more);
    p
}

async fn search_in(tx: &Tx, q: &SearchQuery) -> Result<SearchResults> {
    let (opts, cap) = (headline_options(), MAX_OFFSET + 1);
    let rows = tx
        .query(&page(), &bind(q, true, &[&opts, &q.limit, &q.offset]))
        .await?;
    let hits: Vec<_> = rows.iter().map(hit).collect();
    let total: i64 = tx
        .query_one(&total(), &bind(q, true, &[&cap]))
        .await?
        .get(0);
    let rows = tx.query(&facets(), &bind(q, false, &[&cap])).await?;
    let shown = q.offset + hits.len() as i64;
    let more = hits.len() as i64 == q.limit && shown < total && shown <= MAX_OFFSET;
    Ok(SearchResults {
        next_offset: more.then_some(shown),
        total: total.min(MAX_OFFSET),
        total_capped: total > MAX_OFFSET,
        hits,
        kinds: tally(&rows, |r| Some(r.get(0))),
        agents: tally(&rows, |r| r.get(1)),
    })
}

impl Scoped<'_> {
    /// The steps matching `q` across the runs the caller sees, one page, in one transaction.
    pub async fn search_steps(&self, q: &SearchQuery) -> Result<SearchResults> {
        self.in_tx(async |tx| search_in(tx, q).await).await
    }
}
