//! A run's model-spend limit (`max_cost_usd`, `src/budget.rs`), without a model:
//! - a Claude Code session whose stream shows a call that takes the run past its limit has its
//!   turn stopped at once (the process is not waited for), as a budget failure, and its end
//!   counts the live estimate as its cost;
//! - an `http` agent that reports a cost over the limit ends its run `budget_exceeded`, scored
//!   (a failure), with the limit in `RunStart` and in `GET /api/branches/{id}`.
mod support;
use support::records;
use support::team_of;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{routing::post, Json, Router};
use serde_json::{json, Value};
use shaked::{
    agents::{
        claude::ClaudeDriver,
        session::{SessionStart, Turn},
        AgentResult, Driver, FailureKind,
    },
    exec::BranchCtx,
    model::*,
    store::Store,
};
use support::packages::TestHub;
use support::{app, live, run};

fn member(id: &str, kind: AgentKind, model: &str) -> AgentSpec {
    AgentSpec::new(id, kind).with_model(model).with_prompt("p")
}

/// A stand-in for `claude` (stream-json): on its first input it shows one Haiku call with 50 M
/// cache-read tokens (USD 2.50 at the long-context rate), then works on for a minute.
fn fake_claude(dir: &std::path::Path) -> String {
    let path = dir.join("claude");
    let call = json!({"type": "assistant", "message": {"id": "msg_1", "model": "claude-haiku-5-5",
        "usage": {"input_tokens": 10, "cache_read_input_tokens": 50_000_000, "output_tokens": 5},
        "content": [{"type": "text", "text": "thinking"}]}});
    support::write_script(&path, &format!("read line\necho '{call}'\nsleep 60"));
    path.to_string_lossy().into_owned()
}

/// A live branch of a retail world for one Claude Code agent, its run limited to USD 1.
async fn claude_branch(hub: &TestHub, store: &Arc<Store>) -> (Team, Arc<BranchCtx>) {
    let team = team_of(vec![member("lead", AgentKind::ClaudeCode, "haiku")]);
    let run_id = records(store).create_run("task", &team).await.unwrap();
    let branch = records(store)
        .create_branch(&run_id, None, &team, &json!([]), None)
        .await
        .unwrap();
    let world = hub.start_world("retail", &branch).await;
    let rules = Rules::new(Runner::Dispatch).with_max_cost(Some(1.0));
    let ctx = BranchCtx::new(branch, world, team.clone(), "task".into()).with_rules(rules);
    let ctx = support::in_store(ctx, store);
    (team, Arc::new(ctx))
}

/// One turn of the fake `claude` on `ctx`: how it ended, the session's end, and how long it took.
async fn one_turn(ctx: &Arc<BranchCtx>, spec: AgentSpec) -> (Turn, AgentResult, Duration) {
    let dir = tempfile::tempdir().unwrap();
    let mut driver = ClaudeDriver::new("http://127.0.0.1:9");
    driver.binary = fake_claude(dir.path());
    let start = SessionStart {
        branch: ctx.clone(),
        spec,
    };
    let mut session = driver.session(start).await.unwrap();
    let since = Instant::now();
    let turn = session.turn("go".into()).await;
    let took = since.elapsed();
    let end = session.end(true).await;
    (turn, end, took)
}

#[test]
fn a_claude_turn_over_the_limit_is_stopped_at_once_and_its_estimate_is_its_cost() {
    run(async {
        let hub = TestHub::new(&[]);
        let (store, _clean) = app::meta_store("budget", "claude").await;
        let (team, ctx) = claude_branch(&hub, &store).await;
        let (turn, end, took) = one_turn(&ctx, team.agents[0].clone()).await;
        assert!(took < Duration::from_secs(20), "the turn was waited out");
        assert_eq!(turn.failure, Some(FailureKind::Budget), "{turn:?}");
        let why = turn.error.unwrap();
        assert!(
            why.contains("USD 2.50") && why.contains("USD 1.00"),
            "{why}"
        );
        let cost = end.cost_usd.unwrap();
        assert!((cost - 2.5).abs() < 0.01, "{cost}");
        assert_eq!(end.cost_source.as_deref(), Some("list_price_estimate"));
        let calls = records(&store).events(&ctx.branch_id).await.unwrap();
        assert!(calls.iter().any(|e| e.kind == EventKind::ModelCall));
        ctx.world.stop().await;
    });
}

/// An `http` agent that does nothing and reports USD 0.25.
async fn spender() -> String {
    let app = Router::new().route(
        "/run",
        post(|Json(body): Json<Value>| async move {
            if body["type"] == "hello" {
                return Json(json!({"name": "Spender"}));
            }
            Json(
                json!({"ok": true, "usage": {"input_tokens": 1000, "output_tokens": 10},
                        "cost_usd": 0.25}),
            )
        }),
    );
    let (listener, base) = app::bind().await;
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("{base}/run")
}

async fn post_run(base: &str, body: Value) -> (u16, Value) {
    let req = reqwest::Client::new().post(format!("{base}/api/runs"));
    app::send(req.json(&body)).await
}

async fn get(base: &str, path: &str) -> Value {
    app::send(reqwest::Client::new().get(format!("{base}{path}")))
        .await
        .1
}

#[test]
fn a_run_whose_reported_cost_passes_its_limit_ends_budget_exceeded_and_scored() {
    run(async {
        let hub = TestHub::new(&[]);
        let (store, _clean) = app::meta_store("budget", "http").await;
        let (state, base) = app::idle_app(store, hub.hub.clone(), None, |_| {}).await;
        let url = spender().await;
        let team = json!({"id": "t", "name": "T", "agents": [
            {"id": "remote", "name": "Remote", "kind": "http", "url": url}]});
        let post = |body: Value| post_run(&base, body);
        let (st, v) = post(json!({"task_id": "27", "team": team, "max_cost_usd": 0})).await;
        assert_eq!(st, 400, "{v}");
        let (st, v) = post(json!({"task_id": "27", "team": team, "max_cost_usd": 0.1})).await;
        assert_eq!(st, 200, "{v}");
        let id = v["branch_id"].as_str().unwrap();
        let b = live::ended(&state, id).await;
        assert_eq!(b.status, BranchStatus::BudgetExceeded, "{:?}", b.error);
        assert!(b.error.unwrap().contains("USD 0.25"));
        let evs = records(&state.store).events(id).await.unwrap();
        assert!(
            evs.iter().any(|e| e.kind == EventKind::Check),
            "scored as it stood"
        );
        let start = evs.iter().find(|e| e.kind == EventKind::RunStart).unwrap();
        assert_eq!(start.payload["max_cost_usd"], 0.1);
        let shown = get(&base, &format!("/api/branches/{id}")).await;
        assert_eq!(shown["budget"]["max_cost_usd"], 0.1);
        assert_eq!(shown["status"], "budget_exceeded");
    });
}

/// An `http` agent that answers its run once `go` is notified: `fail` with HTTP 500, else
/// never.
async fn waiting_agent(go: Arc<tokio::sync::Notify>, fail: bool) -> String {
    let app = Router::new().route(
        "/run",
        post(move |Json(body): Json<Value>| {
            let go = go.clone();
            async move {
                if body["type"] == "hello" {
                    return (axum::http::StatusCode::OK, Json(json!({"name": "Waiter"})));
                }
                go.notified().await;
                if !fail {
                    std::future::pending::<()>().await;
                }
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({})),
                )
            }
        }),
    );
    let (listener, base) = app::bind().await;
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("{base}/run")
}

/// A local run over its estimate: its app (with what drops its database), base and branch.
struct Local {
    state: Arc<shaked::app::AppState>,
    base: String,
    id: String,
    _hub: TestHub,
    _clean: support::pg::Cleanup,
}

/// A local run of the agent at `url` with a limit of USD 0.1, its estimate put past it (by its
/// own count, not the control box's word).
async fn over_its_estimate(url: &str, tag: &str) -> Local {
    let hub = TestHub::new(&[]);
    let (store, clean) = app::meta_store("budget", tag).await;
    let (state, base) = app::idle_app(store, hub.hub.clone(), None, |_| {}).await;
    let team = json!({"id": "t", "name": "T", "agents": [
        {"id": "remote", "name": "Remote", "kind": "http", "url": url}]});
    let body = json!({"task_id": "27", "team": team, "max_cost_usd": 0.1});
    let (st, v) = post_run(&base, body).await;
    assert_eq!(st, 200, "{v}");
    let id = v["branch_id"].as_str().unwrap().to_string();
    let ctx = loop {
        if let Some(ctx) = state.live.get(&id) {
            break ctx;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    ctx.budget.add("remote", 1.0);
    assert!(ctx.budget.over().is_some() && ctx.budget.forced().is_none());
    Local {
        state,
        base,
        id,
        _hub: hub,
        _clean: clean,
    }
}

/// Only the control box's word turns a stop into `budget_exceeded`: a person's stop of a local
/// run stays `stopped`, its estimate past its limit or not.
#[test]
fn a_local_stop_stays_stopped_over_its_estimate() {
    run(async {
        let go = Arc::new(tokio::sync::Notify::new());
        let url = waiting_agent(go.clone(), false).await;
        let run = over_its_estimate(&url, "stop").await;
        let stop = format!("{}/api/branches/{}/stop", run.base, run.id);
        let req = reqwest::Client::new().post(stop);
        assert_eq!(app::send(req.json(&json!({}))).await.0, 200);
        let b = live::ended(&run.state, &run.id).await;
        assert_eq!(b.status, BranchStatus::Stopped, "{:?}", b.error);
    });
}

/// A local agent's genuine infrastructure failure stays `infra_error`, its estimate past its
/// limit or not.
#[test]
fn a_local_infra_failure_stays_infra_error_over_its_estimate() {
    run(async {
        let go = Arc::new(tokio::sync::Notify::new());
        let url = waiting_agent(go.clone(), true).await;
        let run = over_its_estimate(&url, "infra").await;
        go.notify_one();
        let b = live::ended(&run.state, &run.id).await;
        assert_eq!(b.status, BranchStatus::InfraError, "{:?}", b.error);
    });
}
