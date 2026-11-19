//! Save points: a part whose `describe` says `save_at: "points"` is not saved with each step that
//! writes to it, but before the next model call through the gateway, when a driver asks, and at
//! the run's end. The run is one an external system drives: the test makes the agent's calls
//! through its MCP endpoint and gateway; the gateway's provider is a closed port, so no model
//! answers anything here.
mod support;

use arenad::model::*;
use axum::{http::StatusCode, routing::post, Router};
use serde_json::{json, Value};
use support::{app::send, packages, run, AppFx};

/// An app over a toy part `box` started with `part_args`, and a live external run of task `t1`
/// with the agent `planner`.
struct SavesFx {
    fx: AppFx,
    branch: String,
    mcp: String,
    chat: String,
}

impl SavesFx {
    async fn new(tag: &str, part_args: &[&str]) -> SavesFx {
        let root = packages::toy_world(tag, part_args, &[]);
        let fx = AppFx::idle("saves", tag)
            .root(root)
            .bench("toybench")
            .start()
            .await;
        let team = json!({"id": "own", "name": "Own design", "agents": []});
        let body = json!({"task_id": "t1", "team": team, "runner": "external"});
        let (status, v) = fx.post("/api/runs", body).await;
        assert_eq!(status, 200, "{v}");
        let branch = v["branch_id"].as_str().unwrap().to_string();
        let path = format!("/api/branches/{branch}/agents");
        let (status, a) = fx
            .post(&path, json!({"id": "planner", "name": "Planner"}))
            .await;
        assert_eq!(status, 200, "{a}");
        let mcp = a["mcp_url"].as_str().unwrap().to_string();
        let chat = format!("{}/chat/completions", a["gateway_url"].as_str().unwrap());
        SavesFx {
            fx,
            branch,
            mcp,
            chat,
        }
    }

    /// `poke` through the agent's MCP endpoint.
    async fn poke(&self) {
        let req = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                         "params": {"name": "poke", "arguments": {}}});
        let (status, v) = send(self.fx.http.post(&self.mcp).json(&req)).await;
        assert_eq!(status, 200, "{v}");
    }

    /// A model call through the agent's gateway (it fails to reach the provider).
    async fn model_call(&self) {
        let body = json!({"model": "m", "messages": [{"role": "user", "content": "hi"}]});
        self.fx
            .http
            .post(&self.chat)
            .json(&body)
            .send()
            .await
            .unwrap();
    }

    async fn events(&self) -> Vec<Event> {
        self.fx.state.store.events(&self.branch).await.unwrap()
    }

    async fn of_kind(&self, kind: EventKind) -> Vec<Event> {
        let evs = self.events().await;
        evs.into_iter().filter(|e| e.kind == kind).collect()
    }

    async fn on(&self, what: &str) -> (u16, Value) {
        let path = format!("/api/branches/{}/{what}", self.branch);
        self.fx.post(&path, json!({})).await
    }
}

const POINTS: [&str; 2] = ["--save-at", "points"];

#[test]
fn a_write_to_a_points_part_is_not_saved_with_its_step() {
    run(async {
        let fx = SavesFx::new("pointswrite", &POINTS).await;
        fx.poke().await;
        let calls = fx.of_kind(EventKind::ToolCall).await;
        assert_eq!(calls[0].payload["writes"], json!(["box/n"]));
        assert!(
            calls[0].payload.get("saves").is_none(),
            "{}",
            calls[0].payload
        );
        assert!(calls[0].payload.get("snapshot_error").is_none());
    })
}

#[test]
fn a_model_call_after_a_write_saves_the_points_part_first() {
    run(async {
        let fx = SavesFx::new("pointsmodel", &POINTS).await;
        fx.poke().await;
        fx.model_call().await;
        let points = fx.of_kind(EventKind::SavePoint).await;
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].payload["reason"], "model_call");
        assert!(
            points[0].payload["saves"]["box"].is_string(),
            "{}",
            points[0].payload
        );
        let calls = fx.of_kind(EventKind::ModelCall).await;
        assert!(points[0].seq < calls[0].seq);
        fx.model_call().await;
        assert_eq!(fx.of_kind(EventKind::SavePoint).await.len(), 1);
    })
}

/// An upstream that answers `POST /items` with 201.
async fn items_upstream() -> String {
    let app = Router::new().route("/items", post(|| async { (StatusCode::CREATED, "made") }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

#[test]
fn an_http_write_to_a_points_part_waits_for_the_save_point() {
    run(async {
        let route = format!("toy.test={}", items_upstream().await);
        let fx = SavesFx::new(
            "pointshttp",
            &["--save-at", "points", "--http-route", &route],
        )
        .await;
        let ticket = fx.fx.state.proxy.tickets.issue(&fx.branch, Some("planner"));
        let proxy = reqwest::Proxy::all(fx.fx.state.proxy.url(&ticket).unwrap()).unwrap();
        let c = reqwest::Client::builder().proxy(proxy).build().unwrap();
        let r = c
            .post("http://toy.test/items")
            .body("x")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 201);
        let calls = fx.of_kind(EventKind::ToolCall).await;
        assert!(
            calls[0].payload.get("saves").is_none(),
            "{}",
            calls[0].payload
        );
        fx.model_call().await;
        let points = fx.of_kind(EventKind::SavePoint).await;
        assert!(points[0].payload["saves"]["box"].is_string());
    })
}

#[test]
fn asking_for_a_save_records_a_save_point() {
    run(async {
        let fx = SavesFx::new("pointsasked", &POINTS).await;
        fx.poke().await;
        let (status, v) = fx.on("save").await;
        assert_eq!(status, 200, "{v}");
        assert!(v["seq"].is_i64() && v["saves"]["box"].is_string(), "{v}");
        let points = fx.of_kind(EventKind::SavePoint).await;
        assert_eq!(
            (points[0].seq, &points[0].payload["reason"]),
            (v["seq"].as_i64().unwrap(), &json!("asked"))
        );
        assert_eq!(
            fx.on("save").await,
            (200, json!({"seq": null, "saves": {}}))
        );
        assert_eq!(fx.on("finish").await.0, 200);
        assert_eq!(fx.on("save").await.0, 409);
    })
}

#[test]
fn the_run_end_saves_a_points_part_that_changed() {
    run(async {
        let fx = SavesFx::new("pointsend", &POINTS).await;
        fx.poke().await;
        assert_eq!(fx.on("finish").await.0, 200);
        let base = fx.of_kind(EventKind::RunStart).await[0].payload["saves"]["box"].clone();
        let check = fx.of_kind(EventKind::Check).await;
        let last = &check[0].payload["final_saves"]["box"];
        assert!(last.is_string() && *last != base, "{last} vs {base}");
    })
}

#[test]
fn a_step_part_is_still_saved_after_each_write() {
    run(async {
        let fx = SavesFx::new("stepwrite", &[]).await;
        fx.poke().await;
        fx.model_call().await;
        fx.poke().await;
        let calls = fx.of_kind(EventKind::ToolCall).await;
        assert!(calls.iter().all(|c| c.payload["saves"]["box"].is_string()));
        assert!(fx.of_kind(EventKind::SavePoint).await.is_empty());
    })
}

#[test]
fn with_nothing_to_save_a_save_point_never_waits_for_the_gate() {
    run(async {
        let fx = SavesFx::new("nogatewait", &[]).await;
        let ctx = fx.fx.state.live.get(&fx.branch).unwrap();
        let _held = ctx.gate().await; // a long step holds the gate
        let exec = arenad::exec::ToolExec::new(fx.fx.state.store.clone());
        let point = exec.save_point(&ctx, "model_call");
        let made = tokio::time::timeout(std::time::Duration::from_secs(2), point).await;
        assert!(matches!(made, Ok(Ok(None))), "{made:?}");
    })
}
