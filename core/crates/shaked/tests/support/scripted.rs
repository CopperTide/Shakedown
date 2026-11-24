//! A scripted `http` agent (no model) for the tests that run a branch in a worker: it lists its
//! tools, waits for what the test says (`Between`), pokes the toy box once and answers done.
use std::sync::Arc;

use axum::{extract::State, routing::post, Json, Router};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use shaked::model::Team;

use super::app;

/// What the scripted agent does between listing its tools and calling one.
pub type Between = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

/// The scripted agent doing nothing between its calls.
pub fn nothing() -> Between {
    Arc::new(|| Box::pin(async {}))
}

/// An MCP call's answer; Null when the endpoint is gone (its branch was stopped).
pub async fn mcp(url: &str, token: &str, body: Value) -> Value {
    let req = reqwest::Client::new().post(url).bearer_auth(token);
    match req.json(&body).send().await {
        Ok(r) => r.json().await.unwrap_or(Value::Null),
        Err(_) => Value::Null,
    }
}

/// The scripted agent's one turn.
async fn work(State(between): State<Between>, Json(run): Json<Value>) -> Json<Value> {
    if run["type"] == "hello" {
        return Json(json!({}));
    }
    let (url, token) = (
        run["mcp_url"].as_str().unwrap(),
        run["token"].as_str().unwrap(),
    );
    let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    mcp(url, token, list).await;
    between().await;
    let call = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                      "params": {"name": "poke", "arguments": {}}});
    mcp(url, token, call).await;
    Json(json!({"ok": true}))
}

/// The scripted agent served on a free port: the team of it alone.
pub async fn scripted_team(between: Between) -> Team {
    let app = Router::new().route("/run", post(work)).with_state(between);
    let (listener, base) = app::bind().await;
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let agent = json!({"id": "a", "name": "A", "kind": "http", "url": format!("{base}/run")});
    serde_json::from_value(json!({"id": "t", "name": "T", "agents": [agent]})).unwrap()
}
