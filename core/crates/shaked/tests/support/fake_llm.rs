//! A scripted OpenAI-compatible server shaped like vLLM's gpt-oss answers (v0.32.0 `ChatMessage`:
//! the thought in `reasoning`), standing in for a model in tests only.
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::oneshot;

/// A request the server got.
#[derive(Debug, Clone)]
pub struct Seen {
    pub path: String,
    pub authorization: Option<String>,
    pub body: Value,
}

/// The server: its base URL (`…/v1`) and what it was asked. It shuts down on drop.
pub struct FakeLlm {
    pub base: String,
    pub seen: Arc<Mutex<Vec<Seen>>>,
    _stop: oneshot::Sender<()>,
}

#[derive(Clone)]
struct Script {
    answers: Arc<Vec<(u16, Value)>>,
    seen: Arc<Mutex<Vec<Seen>>>,
}

/// A server answering `POST …/v1/chat/completions` with `answers` in order (the last one
/// repeats); a `Value::String` body is sent as `text/html`.
pub async fn fake_llm(answers: Vec<(u16, Value)>) -> FakeLlm {
    let seen = Arc::new(Mutex::new(vec![]));
    let script = Script {
        answers: Arc::new(answers),
        seen: seen.clone(),
    };
    let app = Router::new()
        .route("/v1/chat/completions", post(answer))
        .with_state(script);
    serve(app, seen).await
}

/// Serves `app` on a free port until the server is dropped.
async fn serve(app: Router, seen: Arc<Mutex<Vec<Seen>>>) -> FakeLlm {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", l.local_addr().unwrap());
    let (stop, stopped) = oneshot::channel::<()>();
    let serve = axum::serve(l, app).with_graceful_shutdown(async {
        let _ = stopped.await;
    });
    tokio::spawn(async move { serve.await.unwrap() });
    FakeLlm {
        base,
        seen,
        _stop: stop,
    }
}

/// What a scripted server answers to a request's body.
pub type Answerer = Arc<dyn Fn(&Value) -> Value + Send + Sync>;

/// A server answering `POST …/v1/chat/completions` with what `answerer` gives for each request
/// (200), for agents whose turns depend on what they were asked rather than on the order.
pub async fn fake_llm_with(answerer: Answerer) -> FakeLlm {
    let seen = Arc::new(Mutex::new(vec![]));
    let state = (answerer, seen.clone());
    let app = Router::new()
        .route("/v1/chat/completions", post(answer_with))
        .with_state(state);
    serve(app, seen).await
}

async fn answer_with(
    State((answerer, seen)): State<(Answerer, Arc<Mutex<Vec<Seen>>>)>,
    Json(body): Json<Value>,
) -> Response {
    let out = answerer(&body);
    let path = "/v1/chat/completions".to_string();
    let authorization = None;
    seen.lock().unwrap().push(Seen {
        path,
        authorization,
        body,
    });
    Json(out).into_response()
}

async fn answer(State(s): State<Script>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let authorization = (headers.get("authorization"))
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let n = {
        let mut seen = s.seen.lock().unwrap();
        let path = "/v1/chat/completions".to_string();
        seen.push(Seen {
            path,
            authorization,
            body,
        });
        seen.len() - 1
    };
    let (status, body) = s.answers[n.min(s.answers.len() - 1)].clone();
    let status = StatusCode::from_u16(status).unwrap();
    match body {
        Value::String(html) => (status, [("content-type", "text/html")], html).into_response(),
        v => (status, Json(v)).into_response(),
    }
}

/// A gpt-oss answer as vLLM v0.32.0 gives it: one `shell` tool call, or `content` when given,
/// with `reasoning` and usage 812/64 (768 cached).
pub fn gpt_oss(reasoning: &str, content: Option<&str>) -> Value {
    let message = match content {
        Some(c) => json!({"role": "assistant", "content": c, "reasoning": reasoning}),
        None => json!({"role": "assistant", "content": null, "reasoning": reasoning,
            "tool_calls": [{"id": "chatcmpl-tool-1", "type": "function",
                "function": {"name": "shell", "arguments": "{\"cmd\":\"gh pr list\"}"}}]}),
    };
    let finish = if content.is_some() {
        "stop"
    } else {
        "tool_calls"
    };
    json!({"id": "chatcmpl-1", "object": "chat.completion", "model": "openai/gpt-oss-120b",
        "choices": [{"index": 0, "finish_reason": finish, "message": message}],
        "usage": {"prompt_tokens": 812, "completion_tokens": 64, "total_tokens": 876,
                  "prompt_tokens_details": {"cached_tokens": 768}}})
}
