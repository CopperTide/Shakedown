//! A run's model-spend limit (`max_cost_usd`, `src/budget.rs`), without a model:
//! - a Claude Code session whose stream shows a call that takes the run past its limit has its
//!   turn stopped at once (the process is not waited for), as a budget failure, and its end
//!   counts the live estimate as its cost;
//! - an `http` agent that reports a cost over the limit ends its run `budget_exceeded`, scored
//!   (a failure), with the limit in `RunStart` and in `GET /api/branches/{id}`.
#[path = "support/app.rs"]
mod app_support;
#[path = "support/live.rs"]
mod live;
#[path = "support/packages.rs"]
mod packages;
#[path = "support/pg.rs"]
mod pg;

use std::{
    future::Future,
    os::unix::fs::PermissionsExt,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use arenad::{
    agents::{
        claude::ClaudeDriver,
        session::{SessionStart, Turn},
        AgentResult, Driver, FailureKind,
    },
    exec::BranchCtx,
    model::*,
    store::Store,
};
use axum::{routing::post, Json, Router};
use packages::TestHub;
use serde_json::{json, Value};

static RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().unwrap());

fn run<F: Future>(f: F) -> F::Output {
    RT.block_on(f)
}

fn member(id: &str, kind: AgentKind, model: &str) -> AgentSpec {
    AgentSpec {
        tools: Vec::new(),
        id: id.into(),
        name: id.into(),
        kind,
        model: Some(model.into()),
        prompt: "p".into(),
        provider: None,
        read_only: false,
        lead: false,
        url: None,
        headers: Default::default(),
    }
}

/// A stand-in for `claude` (stream-json): on its first input it shows one Haiku call with 50 M
/// cache-read tokens (USD 2.50 at the long-context rate), then works on for a minute.
fn fake_claude(dir: &std::path::Path) -> String {
    let path = dir.join("claude");
    let call = json!({"type": "assistant", "message": {"id": "msg_1", "model": "claude-haiku-5-5",
        "usage": {"input_tokens": 10, "cache_read_input_tokens": 50_000_000, "output_tokens": 5},
        "content": [{"type": "text", "text": "thinking"}]}});
    let script = format!("#!/bin/sh\nread line\necho '{call}'\nsleep 60\n");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().into_owned()
}

/// A live branch of a retail world for one Claude Code agent, its run limited to USD 1.
async fn claude_branch(hub: &TestHub, store: &Arc<Store>) -> (Team, Arc<BranchCtx>) {
    let team = Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![member("lead", AgentKind::ClaudeCode, "haiku")],
    };
    let run_id = store.create_run("task", &team).await.unwrap();
    let branch = store
        .create_branch(&run_id, None, &team, &json!([]), None)
        .await
        .unwrap();
    let world = hub.start_world("retail", &branch).await;
    let rules = Rules::new(Runner::Dispatch).with_max_cost(Some(1.0));
    let ctx = BranchCtx::new(branch, world, team.clone(), "task".into()).with_rules(rules);
    (team, Arc::new(ctx))
}

/// One turn of the fake `claude` on `ctx`: how it ended, the session's end, and how long it took.
async fn one_turn(
    ctx: &Arc<BranchCtx>,
    spec: AgentSpec,
    store: &Arc<Store>,
) -> (Turn, AgentResult, Duration) {
    let dir = tempfile_dir("claude");
    let mut driver = ClaudeDriver::new("http://127.0.0.1:9");
    driver.binary = fake_claude(&dir);
    let start = SessionStart {
        branch: ctx.clone(),
        spec,
        store: store.clone(),
    };
    let mut session = driver.session(start).unwrap();
    let since = Instant::now();
    let turn = session.turn("go".into()).await;
    let took = since.elapsed();
    let end = session.end(true).await;
    let _ = std::fs::remove_dir_all(dir);
    (turn, end, took)
}

#[test]
fn a_claude_turn_over_the_limit_is_stopped_at_once_and_its_estimate_is_its_cost() {
    run(async {
        let hub = TestHub::new(&[]);
        let (store, _clean) = app_support::meta_store("budget", "claude").await;
        let (team, ctx) = claude_branch(&hub, &store).await;
        let (turn, end, took) = one_turn(&ctx, team.agents[0].clone(), &store).await;
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
        let calls = store.events(&ctx.branch_id).await.unwrap();
        assert!(calls.iter().any(|e| e.kind == EventKind::ModelCall));
        ctx.world.stop().await;
    });
}

fn tempfile_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("arenad-budget-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
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
    let (listener, base) = app_support::bind().await;
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("{base}/run")
}

async fn post_run(base: &str, body: Value) -> (u16, Value) {
    let req = reqwest::Client::new().post(format!("{base}/api/runs"));
    app_support::send(req.json(&body)).await
}

async fn get(base: &str, path: &str) -> Value {
    app_support::send(reqwest::Client::new().get(format!("{base}{path}")))
        .await
        .1
}

#[test]
fn a_run_whose_reported_cost_passes_its_limit_ends_budget_exceeded_and_scored() {
    run(async {
        let hub = TestHub::new(&[]);
        let (store, _clean) = app_support::meta_store("budget", "http").await;
        let (state, base) = app_support::idle_app(store, hub.hub.clone(), None, |_| {}).await;
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
        let evs = state.store.events(id).await.unwrap();
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
