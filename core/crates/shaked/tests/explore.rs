//! Explore against fixture runs in a fresh app: each model call's usage kept with its branch's
//! figures, the builder's measures by dimensions and its drill-through, read-only SQL behind its
//! walls (guard, read-only transaction, timeout, the explore role), and saved views, all through
//! the operations over HTTP.
mod support;

use serde_json::{json, Value};
use shaked::{
    analytics::{
        self,
        explore::{guard::Checked, sql},
    },
    auth::Caller,
    model::*,
    store::Store,
};
use support::{records, run, team_of, AppFx};

/// A run's steps: agent `a` makes a call and refunds twice (once refused); `b` fails a lookup and
/// makes a call. Each call cost `cost`.
fn steps(cost: f64) -> Vec<(Option<&'static str>, EventKind, Value)> {
    let usage = json!({"prompt_tokens": 100, "completion_tokens": 10, "prompt_tokens_details": {"cached_tokens": 40}});
    let call = json!({"request": {"model": "m1"}, "usage": usage, "cost_usd": cost});
    let tool = |name: &str, error: Option<&str>| json!({"name": name, "args": {}, "error": error});
    vec![
        (
            None,
            EventKind::RunStart,
            json!({"task_id": "refund", "benchmark": "retail"}),
        ),
        (Some("a"), EventKind::ModelCall, call.clone()),
        (Some("a"), EventKind::ToolCall, tool("refund", None)),
        (
            Some("a"),
            EventKind::ToolCall,
            tool("refund", Some("refund refused: needs approval")),
        ),
        (
            Some("b"),
            EventKind::ToolCall,
            tool("lookup", Some("timeout")),
        ),
        (Some("b"), EventKind::ModelCall, call),
    ]
}

/// One run of setup `setup` with agents `a` (model m1) and `b`, ended with `status`.
async fn record(store: &Store, setup: &str, status: BranchStatus, cost: f64) -> (String, String) {
    let s = records(store);
    let agents = vec![
        AgentSpec::new("a", AgentKind::Open),
        AgentSpec::new("b", AgentKind::Open),
    ];
    let mut t = team_of(agents);
    t.name = setup.into();
    t.agents[0].model = Some("m1".into());
    let run = s.create_run("refund", &t).await.unwrap();
    let b = s
        .create_branch(&run, None, &t, &json!([]), None)
        .await
        .unwrap();
    for (agent, kind, payload) in steps(cost) {
        s.append(&b, agent, kind, payload).await.unwrap();
    }
    s.set_status(&b, status, None).await.unwrap();
    (run, b)
}

/// An app with two alpha runs ($0.5 a call, one passed) and one beta run ($1 a call, failed),
/// figures worked out.
async fn app(tag: &str) -> (AppFx, Vec<(String, String)>) {
    let fx = AppFx::idle("explore", tag).start().await;
    let store = &fx.state.store;
    let ids = vec![
        record(store, "alpha", BranchStatus::Passed, 0.5).await,
        record(store, "alpha", BranchStatus::Failed, 0.5).await,
        record(store, "beta", BranchStatus::Failed, 1.0).await,
    ];
    assert_eq!(analytics::fill(store, 100).await.unwrap(), 3);
    (fx, ids)
}

async fn query(fx: &AppFx, spec: Value) -> Value {
    let (code, out) = fx.post("/api/explore/query", spec).await;
    assert_eq!(code, 200, "{out}");
    out
}

#[test]
fn measures_by_run_and_step_dimensions() {
    run(async {
        let (fx, _) = app("query").await;
        let out = query(
            &fx,
            json!({"measures": ["runs", "pass_rate", "cost"], "dimensions": ["setup"]}),
        )
        .await;
        assert_eq!(out["columns"][0]["role"], "dimension");
        assert_eq!(
            out["rows"],
            json!([["alpha", 2.0, 0.5, 2.0], ["beta", 1.0, 0.0, 2.0]])
        );
        let out = query(
            &fx,
            json!({"measures": ["refusals", "errors", "tool_calls"], "dimensions": ["tool"]}),
        )
        .await;
        assert_eq!(out["rows"][0], json!(["refund", 3.0, 0.0, 6.0]), "{out}");
        assert_eq!(out["rows"][1], json!(["lookup", 0.0, 3.0, 3.0]), "{out}");
        let out = query(&fx, json!({"measures": ["tokens_in", "tokens_cached", "model_calls"], "dimensions": ["agent"],
                                    "filters": {"agent": ["a"]}})).await;
        assert_eq!(out["rows"], json!([["a", 300.0, 120.0, 3.0]]));
        let (code, err) = fx
            .post("/api/explore/query", json!({"measures": ["colour"]}))
            .await;
        assert_eq!(code, 400, "{err}");
    })
}

#[test]
fn a_cell_drills_to_its_runs_or_its_steps() {
    run(async {
        let (fx, ids) = app("drill").await;
        let spec = json!({"measures": ["runs"], "dimensions": ["setup"]});
        let (code, out) = fx
            .post(
                "/api/explore/drill",
                json!({"spec": spec, "cell": {"setup": "beta"}}),
            )
            .await;
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["kind"], "runs");
        assert_eq!(out["runs"][0]["branch_id"], ids[2].1.as_str());
        let spec = json!({"measures": ["refusals"], "dimensions": ["tool"]});
        let cell = json!({"spec": spec, "cell": {"tool": "refund"}, "measure": "refusals"});
        let (_, out) = fx.post("/api/explore/drill", cell).await;
        assert_eq!(out["kind"], "steps");
        let steps = out["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 3);
        assert!(
            steps
                .iter()
                .all(|s| s["seq"] == 4 && s["outcome"] == "refused"),
            "{out}"
        );
    })
}

async fn sql_of(fx: &AppFx, q: &str) -> (u16, Value) {
    fx.post("/api/explore/sql", json!({ "sql": q })).await
}

/// `q`'s rows over HTTP, asserting it ran.
async fn rows_of(fx: &AppFx, q: &str) -> Value {
    let (code, out) = sql_of(fx, q).await;
    assert_eq!(code, 200, "{q}: {out}");
    out["rows"].clone()
}

#[test]
fn sql_reads_the_explore_views() {
    run(async {
        let (fx, _) = app("sql").await;
        let q =
            "SELECT agent, sum(cost_usd) AS cost FROM model_calls GROUP BY agent ORDER BY agent";
        assert_eq!(sql_of(&fx, q).await.1["columns"][1]["kind"], "number");
        assert_eq!(rows_of(&fx, q).await, json!([["a", 2], ["b", 2]]));
        let q = "SELECT tool, outcome, count(*) FROM tool_calls GROUP BY 1, 2 ORDER BY 1, 2";
        assert_eq!(rows_of(&fx, q).await.as_array().unwrap().len(), 3);
        let q = "SELECT count(*) FROM runs r JOIN agents a USING (branch_id) WHERE r.passed";
        assert_eq!(rows_of(&fx, q).await, json!([[2]]));
        for q in [
            "DELETE FROM runs",
            "SELECT * FROM shakedown.provider_keys",
            "SELECT pg_sleep(1)",
            "SELECT 1; SELECT 2",
        ] {
            let (code, why) = sql_of(&fx, q).await;
            assert_eq!(code, 400, "{q}: {why}");
        }
    })
}

#[test]
fn behind_the_guard_sql_is_read_only_timed_and_runs_as_the_explore_role() {
    run(async {
        let (fx, _) = app("walls").await;
        let (store, me) = (&fx.state.store, Caller::person());
        let raw = |q: &str| Checked { sql: q.into() };
        let a = sql::run_checked(
            store,
            &me,
            raw("SELECT current_user::text, current_setting('transaction_read_only')"),
        )
        .await
        .unwrap();
        assert_eq!(a.rows, vec![json!(["shakedown_explore", "on"])]);
        let e = sql::run_checked(store, &me, raw("SELECT * FROM shakedown.provider_keys"))
            .await
            .unwrap_err();
        assert_eq!(e.status.as_u16(), 400, "{e:?}");
        let e = sql::run_checked(store, &me, raw("SELECT pg_sleep(30)"))
            .await
            .unwrap_err();
        assert!(e.message.contains("longer than 5 s"), "{}", e.message);
    })
}

#[test]
fn saved_views_are_kept_checked_and_deleted() {
    run(async {
        let (fx, _) = app("views").await;
        let spec = json!({"measures": ["cost"], "dimensions": ["agent"], "range": "30d"});
        let (code, v) = fx
            .post(
                "/api/explore/views",
                json!({"name": "Cost by agent", "kind": "builder", "spec": spec}),
            )
            .await;
        assert_eq!(code, 200, "{v}");
        let id = v["id"].as_str().unwrap().to_string();
        let sql = json!({"sql": "SELECT 1"});
        let (code, _) = fx
            .post(
                "/api/explore/views",
                json!({"id": id, "name": "One", "kind": "sql", "spec": sql}),
            )
            .await;
        assert_eq!(code, 200);
        let (_, list) = fx.get("/api/explore/views").await;
        assert_eq!(list["items"][0]["name"], "One");
        let bad = json!({"name": "x", "kind": "sql", "spec": {"sql": "DROP TABLE runs"}});
        assert_eq!(fx.post("/api/explore/views", bad).await.0, 400);
        assert_eq!(fx.delete(&format!("/api/explore/views/{id}")).await.0, 200);
        assert_eq!(fx.get(&format!("/api/explore/views/{id}")).await.0, 404);
    })
}

#[test]
fn usage_is_kept_per_call_and_goes_with_its_run() {
    run(async {
        let (fx, ids) = app("usage").await;
        let (_, schema) = fx.get("/api/explore/schema").await;
        assert_eq!(schema["options"]["setups"], json!(["alpha", "beta"]));
        assert_eq!(schema["sql"]["views"][0]["name"], "runs");
        let (_, out) = sql_of(&fx, "SELECT count(*), sum(input_tokens) FROM model_calls").await;
        assert_eq!(out["rows"], json!([[6, 600]]));
        records(&fx.state.store)
            .delete_run(&ids[0].0)
            .await
            .unwrap();
        let (_, out) = sql_of(&fx, "SELECT count(*) FROM model_calls").await;
        assert_eq!(out["rows"], json!([[4]]));
    })
}
