//! A scripted upstream for the gateway's pass-through routes, standing in for the providers in
//! tests only: Anthropic Messages (`POST /v1/messages`, one JSON message, or streamed when
//! asked; `metadata.fake: error` says `error` after its first event) and its token count, OpenAI
//! Responses (`POST /v1/responses`, streamed; model `cut` breaks off after its first event,
//! `metadata.fake: short` ends without its final one) and chat completions, the last two with an OpenRouter-style
//! reported cost. It keeps what it was asked, headers included. A streamed answer sends its first
//! event, then waits on `gate` for the rest.
use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use shaked::{
    providers::{Endpoint, ProviderConfig},
    vault::Vault,
};
use tokio::sync::Semaphore;

use super::AppFx;

const VAULT: &str = "0001020304050607080910111213141516171819202122232425262728293031";
/// The org's stored Claude key.
pub const ORG_CLAUDE_KEY: &str = "sk-ant-org-key-1111";
/// The org's stored OpenAI key.
pub const ORG_OPENAI_KEY: &str = "sk-org-openai-2222";
/// The org's stored OpenRouter key.
pub const ORG_ROUTER_KEY: &str = "sk-or-org-key-3333";
/// What OpenRouter reports each chat or Responses call cost.
pub const CALL_COST: f64 = 0.004;

/// A request the upstream got.
#[derive(Clone)]
pub struct Seen {
    pub path: String,
    pub headers: HeaderMap,
    pub body: Value,
}

/// The upstream: its base (no `/v1`), what it was asked, and the gate of its streamed answers.
#[derive(Clone)]
pub struct FakeApis {
    pub base: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    pub gate: Arc<Semaphore>,
}

impl FakeApis {
    fn saw(&self, path: &str, headers: HeaderMap, body: Value) {
        let path = path.to_string();
        let seen = Seen {
            path,
            headers,
            body,
        };
        self.seen.lock().unwrap().push(seen);
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    /// Its Anthropic, OpenAI and OpenRouter endpoints, with no key.
    pub fn providers(&self) -> ProviderConfig {
        let base = format!("{}/v1", self.base);
        ProviderConfig {
            anthropic: Endpoint::new(&base, None),
            openai: Endpoint::new(&base, None),
            openrouter: Endpoint::new(&base, None),
            ..ProviderConfig::default()
        }
    }
}

/// Claude's answer to every Messages call.
pub fn claude_message() -> Value {
    json!({"id": "msg_1", "type": "message", "role": "assistant", "model": "claude-sonnet-5-5",
           "content": [{"type": "text", "text": "hello from claude"}], "stop_reason": "end_turn",
           "usage": {"input_tokens": 12, "cache_read_input_tokens": 30,
                     "cache_creation_input_tokens": 0, "output_tokens": 5}})
}

/// The answer to every chat-completions call.
pub fn chat_answer() -> Value {
    json!({"id": "c1", "object": "chat.completion",
           "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"},
                        "finish_reason": "stop"}],
           "usage": {"prompt_tokens": 9, "completion_tokens": 2, "total_tokens": 11,
                     "cost": CALL_COST}})
}

/// The first event of a streamed Messages answer, sent before the gate.
pub const MESSAGE_START: &str = "event: message_start\ndata: {\"type\":\"message_start\",\
    \"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\
    \"model\":\"claude-sonnet-5-5\",\"content\":[],\"stop_reason\":null,\"usage\":\
    {\"input_tokens\":12,\"cache_read_input_tokens\":30,\"cache_creation_input_tokens\":0,\
    \"output_tokens\":1}}}\n\n";
/// The rest of it: the text in two deltas, the stop reason and the output's tokens.
pub const MESSAGE_REST: &str = "event: content_block_start\ndata: {\"type\":\
    \"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\
    \n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\
    \"delta\":{\"type\":\"text_delta\",\"text\":\"hello from \"}}\n\nevent: \
    content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":\
    {\"type\":\"text_delta\",\"text\":\"claude\"}}\n\nevent: content_block_stop\ndata: \
    {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\
    \"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":\
    5}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";

/// The first event of the streamed Responses answer, sent before the gate.
pub const CREATED: &str = "event: response.created\ndata: {\"type\":\"response.created\",\
    \"response\":{\"id\":\"resp_1\",\"status\":\"in_progress\"}}\n\n";
/// The rest of it, sent once the gate lets it.
pub const COMPLETED: &str = "event: response.output_text.delta\ndata: {\"type\":\
    \"response.output_text.delta\",\"delta\":\"hi\"}\n\nevent: response.completed\ndata: \
    {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\
    \"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"hi\"}]}],\
    \"usage\":{\"input_tokens\":20,\"input_tokens_details\":{\"cached_tokens\":4},\
    \"output_tokens\":7,\"total_tokens\":27,\"cost\":0.004}}}\n\n";

/// What follows a streamed Messages answer's first event: an `error` event, then the end, when
/// the request's `metadata.fake` is `error`; else the rest of the message.
const MESSAGE_ERROR: &str = "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\
    \"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";

async fn messages(State(u): State<FakeApis>, h: HeaderMap, Json(body): Json<Value>) -> Response {
    let stream = body["stream"] == true;
    let rest = match body["metadata"]["fake"] == "error" {
        true => MESSAGE_ERROR,
        false => MESSAGE_REST,
    };
    u.saw("/v1/messages", h, body);
    match stream {
        true => streamed(&u, MESSAGE_START, Ok(rest)),
        false => Json(claude_message()).into_response(),
    }
}

async fn count_tokens(State(u): State<FakeApis>, h: HeaderMap, Json(b): Json<Value>) -> Response {
    u.saw("/v1/messages/count_tokens", h, b);
    Json(json!({"input_tokens": 7})).into_response()
}

async fn chat(State(u): State<FakeApis>, h: HeaderMap, Json(body): Json<Value>) -> Response {
    u.saw("/v1/chat/completions", h, body);
    Json(chat_answer()).into_response()
}

/// A streamed Responses answer: `CREATED`, then `COMPLETED` once the gate lets it; for the model
/// `cut` a broken connection instead, and for `metadata.fake` `short` the end with nothing more.
async fn responses(State(u): State<FakeApis>, h: HeaderMap, Json(body): Json<Value>) -> Response {
    let rest = match (body["model"] == "cut", body["metadata"]["fake"] == "short") {
        (true, _) => Err(()),
        (false, true) => Ok(""),
        (false, false) => Ok(COMPLETED),
    };
    u.saw("/v1/responses", h, body);
    streamed(&u, CREATED, rest)
}

/// An SSE answer: `first`, then once the gate lets it `rest`, or a broken stream for `Err`.
fn streamed(u: &FakeApis, first: &'static str, rest: Result<&'static str, ()>) -> Response {
    let gate = u.gate.clone();
    let chunks = futures_util::stream::unfold(0, move |i| {
        let gate = gate.clone();
        async move {
            let chunk = match i {
                0 => Ok(first.to_string()),
                1 => {
                    gate.acquire().await.unwrap().forget();
                    rest.map(str::to_string)
                        .map_err(|()| std::io::Error::other("the provider broke off"))
                }
                _ => return None,
            };
            Some((chunk, i + 1))
        }
    });
    let sse = [("content-type", "text/event-stream")];
    (sse, Body::from_stream(chunks)).into_response()
}

/// The upstream on a free port, its gate shut.
pub async fn fake_apis() -> FakeApis {
    let (listener, base) = super::app::bind().await;
    let u = FakeApis {
        base,
        seen: Arc::default(),
        gate: Arc::new(Semaphore::new(0)),
    };
    let app = Router::new()
        .route("/v1/messages", post(messages))
        .route("/v1/messages/count_tokens", post(count_tokens))
        .route("/v1/responses", post(responses))
        .route("/v1/chat/completions", post(chat))
        .with_state(u.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    u
}

/// The test vault the org's keys are sealed with (never the repository's key file).
pub fn test_vault() -> Arc<Vault> {
    Arc::new(Vault::from_hex(VAULT).unwrap())
}

/// Gives the app a test vault and stores the org's Claude, OpenAI and OpenRouter keys in it.
pub async fn store_org_keys(fx: &AppFx) {
    let _ = fx.state.vault.set(test_vault());
    for (name, kind, secret) in [
        ("org-claude", "anthropic", ORG_CLAUDE_KEY),
        ("org-openai", "openai", ORG_OPENAI_KEY),
        ("org-router", "openrouter", ORG_ROUTER_KEY),
    ] {
        let key = json!({"name": name, "kind": kind, "secret": secret});
        let (status, v) = fx.post("/api/models/keys", key).await;
        assert_eq!(status, 200, "{v}");
    }
}
