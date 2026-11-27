//! The SQL of a builder [`Plan`], made from the catalog's fixed fragments (nothing the caller sends
//! is ever spliced in: names are looked up, values are bound). The runs CTE `r` binds `$1`–`$5`
//! (from, to, experiments, setups, agents); a drill binds its cell's values from `$6`.
//!
//! ```text
//!  r (runs, one row each) ──► s (their steps, model-call usage joined) ──► p (per run and step
//!  dimension: partial sums) ──► r [LEFT] JOIN p, grouped by the dimensions: the measures
//! ```

use super::{
    catalog::{Dimension, Level, Measure},
    spec::Plan,
};

/// The rows a drill returns at most (one more is read, to tell it was cut).
pub const DRILL_CAP: i64 = 200;

/// The runs counted: each run's first branch with current figures (`current` from the store),
/// filtered.
fn runs(current: &str) -> String {
    format!(
        "SELECT b.id AS branch_id, b.run_id, coalesce(rr.name, rr.task_id) AS name, b.created_at AS at, b.status,
       b.status IN ('passed', 'failed', 'budget_exceeded') AS judged, b.status = 'passed' AS passed,
       (b.stats->>'seconds')::float8 AS seconds,
       CASE WHEN jsonb_typeof(b.stats->'cost_usd') = 'number' THEN (b.stats->>'cost_usd')::float8 END AS cost,
       nullif(b.stats->>'world', '') AS world, b.stats->>'scenario' AS scenario, b.stats->>'setup' AS setup,
       x.experiment_id,
       (SELECT jsonb_object_agg(a->>'id', a->>'model') FROM jsonb_array_elements({AGENTS}) a
         WHERE a->>'id' IS NOT NULL) AS agent_models
  FROM branches b JOIN runs rr ON rr.id = b.run_id
  LEFT JOIN LATERAL (SELECT s.experiment_id FROM suite_runs sr JOIN suites s ON s.id = sr.suite_id
                      WHERE sr.run_id = rr.id AND s.experiment_id IS NOT NULL LIMIT 1) x ON true
 WHERE b.parent_id IS NULL AND {current}
   AND ($1::timestamptz IS NULL OR b.created_at >= $1)
   AND ($2::timestamptz IS NULL OR b.created_at < $2)
   AND (cardinality($3::text[]) = 0 OR x.experiment_id = ANY($3))
   AND (cardinality($4::text[]) = 0 OR b.stats->>'setup' = ANY($4))
   AND (cardinality($5::text[]) = 0 OR EXISTS (SELECT 1 FROM jsonb_array_elements({AGENTS}) a
                                                WHERE a->>'id' = ANY($5)))"
    )
}

/// A branch's agents as a JSON array, whatever its team holds.
const AGENTS: &str =
    "CASE WHEN jsonb_typeof(b.team->'agents') = 'array' THEN b.team->'agents' ELSE '[]'::jsonb END";

/// The counted runs' steps (bookkeeping left out), of the agents filtered by, with each model
/// call's usage. Model-call bodies are never read: the payload is touched for tool calls and turn
/// ends only.
const STEPS: &str = "SELECT e.branch_id, e.seq, e.at, e.agent, e.kind,
       shakedown.step_tool(e.kind, e.payload) AS tool, shakedown.step_outcome(e.kind, e.payload) AS outcome,
       coalesce(u.model, r.agent_models->>e.agent) AS model, u.input_tokens, u.output_tokens,
       u.cached_tokens, u.cost_usd, u.think_ms
  FROM r JOIN events e ON e.branch_id = r.branch_id
  LEFT JOIN call_usage u ON u.branch_id = e.branch_id AND u.seq = e.seq
 WHERE e.kind NOT IN ('world_view', 'save_point', 'spans')
   AND (cardinality($5::text[]) = 0 OR e.agent = ANY($5))";

/// What each group of steps adds up to, for the step-level measures.
const PARTIALS: &str = "count(*) AS steps,
       count(*) FILTER (WHERE s.kind = 'model_call') AS model_calls,
       count(*) FILTER (WHERE s.kind = 'tool_call') AS tool_calls,
       count(*) FILTER (WHERE s.outcome = 'refused') AS refusals,
       count(*) FILTER (WHERE s.outcome = 'error') AS errors,
       sum(s.input_tokens) AS tokens_in, sum(s.output_tokens) AS tokens_out,
       sum(s.cached_tokens) AS tokens_cached, sum(s.cost_usd) AS cost, sum(s.think_ms) / 1000.0 AS think_s";

fn step_dims(plan: &Plan) -> impl Iterator<Item = &&'static Dimension> {
    plan.dimensions.iter().filter(|d| d.level == Level::Step)
}

/// The `WITH` of a plan: `r`, and `s` and `p` when it reads steps.
fn with(plan: &Plan, current: &str) -> String {
    let mut sql = format!("WITH r AS ({})", runs(current));
    if plan.needs_steps() {
        let dims: Vec<String> = step_dims(plan)
            .map(|d| format!("{} AS d_{}, ", d.sql, d.id))
            .collect();
        let group: Vec<String> = (1..=dims.len() + 1).map(|i| i.to_string()).collect();
        sql.push_str(&format!(
            ",\ns AS ({STEPS}),\np AS (SELECT {}s.branch_id, {PARTIALS} FROM s GROUP BY {})",
            dims.concat(),
            group.join(", ")
        ));
    }
    sql
}

/// A dimension as the outer query reads it.
fn outer(d: &Dimension) -> String {
    match d.level {
        Level::Run => d.sql.to_string(),
        Level::Step => format!("p.d_{}", d.id),
    }
}

/// The plan's query: one row per group, dimensions then measures, one row past the limit.
pub fn query_sql(plan: &Plan, current: &str) -> String {
    let dims: Vec<String> = plan.dimensions.iter().map(|d| outer(d)).collect();
    let cols: Vec<String> = dims
        .iter()
        .cloned()
        .chain(plan.measures.iter().map(|m| m.sql.to_string()))
        .collect();
    let from = match (plan.needs_steps(), plan.has_step_dimension()) {
        (false, _) => "r",
        (true, false) => "r LEFT JOIN p ON p.branch_id = r.branch_id",
        (true, true) => "r JOIN p ON p.branch_id = r.branch_id",
    };
    let group = match dims.len() {
        0 => String::new(),
        n => format!(
            " GROUP BY {}",
            (1..=n)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    format!(
        "{}\nSELECT {} FROM {from}{group} ORDER BY {} LIMIT {}",
        with(plan, current),
        cols.join(", "),
        order(plan),
        plan.limit + 1
    )
}

/// By day when it is a dimension, else by the first measure, largest first; then the dimensions.
fn order(plan: &Plan) -> String {
    let n = plan.dimensions.len();
    let dims = (1..=n).map(|i| format!("{i} NULLS LAST"));
    let lead = match plan.dimensions.iter().position(|d| d.id == "day") {
        Some(i) => format!("{} ASC", i + 1),
        None => format!("{} DESC NULLS LAST", n + 1),
    };
    std::iter::once(lead)
        .chain(dims)
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a drill lists: the steps behind a cell, or its runs.
pub fn drills_steps(cell: &[&Dimension], measure: Option<&Measure>) -> bool {
    measure.is_some_and(|m| m.level == Level::Step) || cell.iter().any(|d| d.level == Level::Step)
}

/// The rows behind one cell: its dimensions' values are `$6`… in `cell` order.
pub fn drill_sql(cell: &[&Dimension], measure: Option<&Measure>, current: &str) -> String {
    let mut conds: Vec<String> = cell
        .iter()
        .enumerate()
        .map(|(i, d)| format!("{} IS NOT DISTINCT FROM ${}::text", d.sql, i + 6))
        .collect();
    let limit = DRILL_CAP + 1;
    if !drills_steps(cell, measure) {
        conds.push("true".into());
        return format!(
            "WITH r AS ({})\nSELECT r.run_id, r.branch_id, r.name, r.setup, r.scenario, r.world, r.status, r.at,
       r.seconds, r.cost FROM r WHERE {} ORDER BY r.at DESC LIMIT {limit}",
            runs(current),
            conds.join(" AND ")
        );
    }
    conds.push(measure.map_or("true", |m| m.steps).to_string());
    format!(
        "WITH r AS ({}),\ns AS ({STEPS})\nSELECT r.run_id, s.branch_id, r.name, s.seq, s.at, s.agent, s.kind, s.tool,
       s.outcome, s.model FROM s JOIN r ON r.branch_id = s.branch_id WHERE {}
 ORDER BY s.at DESC, s.seq DESC LIMIT {limit}",
        runs(current),
        conds.join(" AND ")
    )
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
