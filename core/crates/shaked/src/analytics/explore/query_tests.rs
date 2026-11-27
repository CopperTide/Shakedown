use serde_json::json;

use super::*;
use crate::analytics::explore::{catalog, spec::Spec};

fn plan(v: serde_json::Value) -> Plan {
    serde_json::from_value::<Spec>(v)
        .unwrap()
        .resolve()
        .unwrap()
}

#[test]
fn run_level_queries_never_read_steps() {
    let sql = query_sql(
        &plan(json!({"measures": ["runs", "pass_rate"], "dimensions": ["setup"]})),
        "true",
    );
    assert!(!sql.contains("events"), "{sql}");
    assert!(
        sql.contains("FROM r GROUP BY 1 ORDER BY 2 DESC NULLS LAST, 1 NULLS LAST LIMIT 501"),
        "{sql}"
    );
}

#[test]
fn step_measures_by_run_dimensions_keep_runs_without_steps() {
    let sql = query_sql(
        &plan(json!({"measures": ["cost"], "dimensions": ["day"], "limit": 10})),
        "true",
    );
    assert!(sql.contains("r LEFT JOIN p"), "{sql}");
    assert!(sql.contains("FROM s GROUP BY 1)"), "{sql}");
    assert!(
        sql.contains("ORDER BY 1 ASC, 1 NULLS LAST LIMIT 11"),
        "{sql}"
    );
}

#[test]
fn step_dimensions_group_the_steps_per_run() {
    let sql = query_sql(
        &plan(json!({"measures": ["refusals"], "dimensions": ["tool", "setup"]})),
        "true",
    );
    assert!(sql.contains("s.tool AS d_tool, s.branch_id"), "{sql}");
    assert!(
        sql.contains(
            "SELECT p.d_tool, r.setup, coalesce(sum(p.refusals), 0)::float8 FROM r JOIN p"
        ),
        "{sql}"
    );
}

#[test]
fn a_drill_binds_its_cell_and_filters_steps_by_the_measure() {
    let tool = catalog::dimension("tool").unwrap();
    let refusals = catalog::measure("refusals");
    let sql = drill_sql(&[tool], refusals, "true");
    assert!(
        sql.contains("s.tool IS NOT DISTINCT FROM $6::text AND s.outcome = 'refused'"),
        "{sql}"
    );
    let setup = catalog::dimension("setup").unwrap();
    assert!(!drills_steps(&[setup], catalog::measure("runs")));
    assert!(drill_sql(&[setup], None, "true")
        .contains("FROM r WHERE r.setup IS NOT DISTINCT FROM $6::text"));
}
