//! Explore's records, as the caller: a builder query and its drill-through (the SQL from
//! `analytics::explore::query`, bound here), the filter options, the explore views' descriptions,
//! and saved views. Every read joins a branch to its run in the caller's transaction, so a caller
//! sees only the runs it may.

use std::{collections::BTreeMap, time::Instant};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use tokio_postgres::{types::ToSql, Row};

use super::{analytics::current, new_id, Scoped, Tx};
use crate::analytics::explore::{
    catalog::{Dimension, Measure},
    docs::{self, ViewDoc},
    query::{self, DRILL_CAP},
    Plan, SCHEMA,
};

/// How long a builder query or a drill may run.
pub const QUERY_TIMEOUT_SECS: u64 = 10;

/// One column of a builder result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutColumn {
    pub id: &'static str,
    pub label: &'static str,
    /// `dimension` or `measure`.
    pub role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<&'static str>,
}

/// What a builder query found: dimensions then measures, one row per group.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryResult {
    pub columns: Vec<OutColumn>,
    pub rows: Vec<Vec<Value>>,
    /// Display names of id-valued dimensions: `experiment` → id → name.
    pub labels: BTreeMap<&'static str, BTreeMap<String, String>>,
    /// True when there were more groups than the limit.
    pub truncated: bool,
    pub ms: u64,
}

/// A run behind a cell.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DrillRun {
    pub run_id: String,
    pub branch_id: String,
    pub name: String,
    pub setup: Option<String>,
    pub scenario: Option<String>,
    pub world: Option<String>,
    pub status: String,
    pub at: DateTime<Utc>,
    pub seconds: Option<f64>,
    pub cost_usd: Option<f64>,
}

/// A step behind a cell (the run page's `?step=`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DrillStep {
    pub run_id: String,
    pub branch_id: String,
    pub run_name: String,
    pub seq: i64,
    pub at: DateTime<Utc>,
    pub agent: Option<String>,
    pub kind: String,
    pub tool: Option<String>,
    pub outcome: Option<String>,
    pub model: Option<String>,
}

/// The runs or steps behind a cell, newest first, at most [`DRILL_CAP`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Drill {
    Runs { runs: Vec<DrillRun>, capped: bool },
    Steps { steps: Vec<DrillStep>, capped: bool },
}

/// The values the builder's filters can take.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ExploreOptions {
    pub experiments: Vec<Value>,
    pub setups: Vec<String>,
    pub agents: Vec<String>,
}

/// A saved view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SavedView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub spec: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

type Param<'a> = &'a (dyn ToSql + Sync);

/// `$1`–`$5`: the plan's filters.
fn filters(p: &Plan) -> Vec<Param<'_>> {
    vec![&p.from, &p.to, &p.experiment, &p.setup, &p.agent]
}

async fn timed(tx: &Tx) -> Result<()> {
    tx.batch(&format!(
        "SET LOCAL statement_timeout = '{QUERY_TIMEOUT_SECS}s'"
    ))
    .await
}

fn out_columns(p: &Plan) -> Vec<OutColumn> {
    let dims = p.dimensions.iter().map(|d| OutColumn {
        id: d.id,
        label: d.label,
        role: "dimension",
        unit: None,
    });
    let measures = p.measures.iter().map(|m| OutColumn {
        id: m.id,
        label: m.label,
        role: "measure",
        unit: Some(m.unit),
    });
    dims.chain(measures).collect()
}

fn out_row(r: &Row, dims: usize, measures: usize) -> Vec<Value> {
    let d = (0..dims).map(|i| json!(r.get::<_, Option<String>>(i)));
    let m = (dims..dims + measures).map(|i| json!(r.get::<_, Option<f64>>(i)));
    d.chain(m).collect()
}

/// The names of the experiments a result's `experiment` column holds.
async fn experiment_labels(
    tx: &Tx,
    p: &Plan,
    rows: &[Vec<Value>],
) -> Result<BTreeMap<String, String>> {
    let Some(i) = p.dimensions.iter().position(|d| d.id == "experiment") else {
        return Ok(BTreeMap::new());
    };
    let ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r[i].as_str().map(str::to_string))
        .collect();
    let found = tx
        .query(
            "SELECT id, name FROM experiments WHERE id = ANY($1)",
            &[&ids],
        )
        .await?;
    Ok(found.iter().map(|r| (r.get(0), r.get(1))).collect())
}

async fn query_in(tx: &Tx, p: &Plan) -> Result<QueryResult> {
    let started = Instant::now();
    timed(tx).await?;
    let found = tx
        .query(&query::query_sql(p, &current()), &filters(p))
        .await?;
    let (n, m) = (p.dimensions.len(), p.measures.len());
    let mut rows: Vec<Vec<Value>> = found.iter().map(|r| out_row(r, n, m)).collect();
    let truncated = rows.len() as i64 > p.limit;
    rows.truncate(p.limit as usize);
    let mut labels = BTreeMap::new();
    let experiments = experiment_labels(tx, p, &rows).await?;
    if !experiments.is_empty() {
        labels.insert("experiment", experiments);
    }
    Ok(QueryResult {
        columns: out_columns(p),
        rows,
        labels,
        truncated,
        ms: started.elapsed().as_millis() as u64,
    })
}

fn drill_run(r: &Row) -> DrillRun {
    DrillRun {
        run_id: r.get(0),
        branch_id: r.get(1),
        name: r.get(2),
        setup: r.get(3),
        scenario: r.get(4),
        world: r.get(5),
        status: r.get(6),
        at: r.get(7),
        seconds: r.get(8),
        cost_usd: r.get(9),
    }
}

fn drill_step(r: &Row) -> DrillStep {
    DrillStep {
        run_id: r.get(0),
        branch_id: r.get(1),
        run_name: r.get(2),
        seq: r.get(3),
        at: r.get(4),
        agent: r.get(5),
        kind: r.get(6),
        tool: r.get(7),
        outcome: r.get(8),
        model: r.get(9),
    }
}

async fn drill_in(
    tx: &Tx,
    p: &Plan,
    cell: &[(&'static Dimension, Option<String>)],
    measure: Option<&Measure>,
) -> Result<Drill> {
    timed(tx).await?;
    let dims: Vec<&Dimension> = cell.iter().map(|(d, _)| *d).collect();
    let sql = query::drill_sql(&dims, measure, &current());
    let mut params = filters(p);
    params.extend(cell.iter().map(|(_, v)| v as Param<'_>));
    let rows = tx.query(&sql, &params).await?;
    let capped = rows.len() as i64 > DRILL_CAP;
    let rows = &rows[..rows.len().min(DRILL_CAP as usize)];
    Ok(if query::drills_steps(&dims, measure) {
        Drill::Steps {
            steps: rows.iter().map(drill_step).collect(),
            capped,
        }
    } else {
        Drill::Runs {
            runs: rows.iter().map(drill_run).collect(),
            capped,
        }
    })
}

/// The experiments, setups and agents of the runs the builder counts.
fn options_sql() -> [String; 3] {
    let (from, at) = (
        "FROM branches b JOIN runs r ON r.id = b.run_id",
        "b.parent_id IS NULL AND",
    );
    let current = current();
    let agents =
        "CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_typeof(b.team->'agents') = 'array'
                    THEN b.team->'agents' ELSE '[]'::jsonb END) a";
    [
        "SELECT x.id, x.name FROM experiments x ORDER BY x.created_at DESC LIMIT 500".into(),
        format!(
            "SELECT DISTINCT b.stats->>'setup' {from} WHERE {at} {current} ORDER BY 1 LIMIT 500"
        ),
        format!(
            "SELECT DISTINCT a->>'id' {from} {agents} WHERE {at} {current} ORDER BY 1 LIMIT 500"
        ),
    ]
}

async fn options_in(tx: &Tx) -> Result<ExploreOptions> {
    let [experiments, setups, agents] = options_sql();
    let names = |rows: Vec<Row>| {
        rows.iter()
            .filter_map(|r| r.get::<_, Option<String>>(0))
            .collect()
    };
    let experiments = tx.query(&experiments, &[]).await?;
    Ok(ExploreOptions {
        experiments: experiments
            .iter()
            .map(|r| json!({"id": r.get::<_, String>(0), "name": r.get::<_, String>(1)}))
            .collect(),
        setups: names(tx.query(&setups, &[]).await?),
        agents: names(tx.query(&agents, &[]).await?),
    })
}

const VIEW_COLS: &str = "id, name, kind, spec, created_at, updated_at";

fn saved_view(r: &Row) -> SavedView {
    SavedView {
        id: r.get(0),
        name: r.get(1),
        kind: r.get(2),
        spec: r.get(3),
        created_at: r.get(4),
        updated_at: r.get(5),
    }
}

impl Scoped<'_> {
    /// The builder's answer to `p`, in one transaction.
    pub async fn explore_query(&self, p: &Plan) -> Result<QueryResult> {
        self.in_tx(async |tx| query_in(tx, p).await).await
    }

    /// The runs or steps behind one cell of `p`'s result: `cell` its dimensions' values.
    pub async fn explore_drill(
        &self,
        p: &Plan,
        cell: &[(&'static Dimension, Option<String>)],
        measure: Option<&Measure>,
    ) -> Result<Drill> {
        self.in_tx(async |tx| drill_in(tx, p, cell, measure).await)
            .await
    }

    pub async fn explore_options(&self) -> Result<ExploreOptions> {
        self.in_tx(async |tx| options_in(tx).await).await
    }

    /// The explore views and their columns, as the database describes them.
    pub async fn explore_views(&self) -> Result<Vec<ViewDoc>> {
        let rows = self.query(docs::DESCRIBE, &[&SCHEMA]).await?;
        Ok(docs::views_of(rows.iter().map(|r| {
            (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4))
        })))
    }

    /// Saved views, last changed first.
    pub async fn saved_views(&self) -> Result<Vec<SavedView>> {
        let sql = format!("SELECT {VIEW_COLS} FROM saved_views ORDER BY updated_at DESC LIMIT 500");
        Ok(self
            .query(&sql, &[])
            .await?
            .iter()
            .map(saved_view)
            .collect())
    }

    pub async fn saved_view(&self, id: &str) -> Result<Option<SavedView>> {
        let sql = format!("SELECT {VIEW_COLS} FROM saved_views WHERE id = $1");
        Ok(self.query_opt(&sql, &[&id]).await?.as_ref().map(saved_view))
    }

    /// Saves a view: a new one without `id`, else replaces view `id` (None when it is gone).
    pub async fn save_view(
        &self,
        id: Option<&str>,
        name: &str,
        kind: &str,
        spec: &Value,
    ) -> Result<Option<SavedView>> {
        let sql = match id {
            None => format!("INSERT INTO saved_views (id, name, kind, spec) VALUES ($1, $2, $3, $4) RETURNING {VIEW_COLS}"),
            Some(_) => format!("UPDATE saved_views SET name = $2, kind = $3, spec = $4, updated_at = now()
                                 WHERE id = $1 RETURNING {VIEW_COLS}"),
        };
        let id = id.map_or_else(new_id, str::to_string);
        Ok(self
            .query_opt(&sql, &[&id, &name, &kind, spec])
            .await?
            .as_ref()
            .map(saved_view))
    }

    /// Deletes view `id`: false when there was none.
    pub async fn delete_view(&self, id: &str) -> Result<bool> {
        Ok(self
            .execute("DELETE FROM saved_views WHERE id = $1", &[&id])
            .await?
            == 1)
    }
}
