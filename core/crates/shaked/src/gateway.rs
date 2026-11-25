//! The recording model gateway, `/gateway/{branch}/{agent}/v1/…`: `chat/completions` (the
//! canonical format shaked's own loop speaks, sent to any provider, [`chat`]), and the
//! pass-through routes `messages` (Anthropic Messages) and `responses` (OpenAI Responses) for CLI
//! agents ([`passthrough`]). Each takes the agent's token, or on the control box its branch's
//! worker token; the provider gets the key chosen for the branch's caller, never the caller's
//! own. In a `shaked worker` every route goes on to the control box ([`forward`]).
//!
//! ```text
//! route ─ target (whom: live agent | linked worker branch)
//!       ─ admit  (held back? save point; body read; layer admits; provider + key)
//!       ─ chat | passthrough → raw_call (sent, answered, a stream relayed)
//!       ─ book   (billed: charge on a linked branch; one model_call; metered) → settle
//! ```
//!
//! The world's own calls (a part's simulated person, the scoring's judge) come in as the actor
//! `world.<who>` with the branch's world token ([`world`]).

mod admit;
mod book;
mod charge;
mod chat;
mod forward;
mod linked;
mod passthrough;
mod raw_call;
mod sse;
mod target;
mod world;

use std::{future::Future, sync::Arc};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};

pub use forward::{router as worker_router, ForwardTo};
pub(crate) use target::live_agent;
use target::Route;

use crate::{
    app::AppState,
    providers::{Provider, ProviderKind, ANTHROPIC, OPENAI},
};

/// The response header that says what a call cost (USD), when that is known: a worker's gateway
/// prices its own loop's calls into the run's estimate with it.
pub const COST_HEADER: &str = "x-shakedown-cost-usd";

/// The API a gateway route speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Api {
    /// OpenAI chat completions, the canonical format.
    Chat,
    /// Anthropic Messages, passed through (Claude Code).
    Messages,
    /// OpenAI Responses, passed through (Codex).
    Responses,
    /// Anthropic's token count of a Messages request, passed through, neither recorded nor
    /// charged (Claude Code asks it).
    CountTokens,
}

/// Every gateway route and the API it speaks.
const ROUTES: [(&str, Api); 4] = [
    ("/gateway/{branch}/{agent}/v1/chat/completions", Api::Chat),
    ("/gateway/{branch}/{agent}/v1/messages", Api::Messages),
    ("/gateway/{branch}/{agent}/v1/responses", Api::Responses),
    (
        "/gateway/{branch}/{agent}/v1/messages/count_tokens",
        Api::CountTokens,
    ),
];

impl Api {
    /// Its path under `…/v1/`.
    pub fn path(self) -> &'static str {
        match self {
            Api::Chat => "chat/completions",
            Api::Messages => "messages",
            Api::Responses => "responses",
            Api::CountTokens => "messages/count_tokens",
        }
    }

    /// The largest request body it takes, read only once the call is let through: the Messages
    /// API's own limit, twice that where requests carry images inline.
    fn max_body(self) -> usize {
        const MIB: usize = 1024 * 1024;
        match self {
            Api::Messages | Api::CountTokens => 32 * MIB,
            Api::Chat | Api::Responses => 64 * MIB,
        }
    }

    /// The provider an agent that names none calls on it (None: the configured default).
    fn default_provider(self) -> Option<&'static str> {
        match self {
            Api::Chat => None,
            Api::Messages | Api::CountTokens => Some(ANTHROPIC),
            Api::Responses => Some(OPENAI),
        }
    }

    /// `p`, when it speaks this API (chat completions go to any provider, translated).
    fn speaking(self, p: Provider) -> Result<Provider, String> {
        let (speaks, name) = match self {
            Api::Chat => (true, ""),
            Api::Messages | Api::CountTokens => (
                p.kind == ProviderKind::Anthropic,
                "the Anthropic Messages API",
            ),
            Api::Responses => (
                matches!(
                    p.kind,
                    ProviderKind::OpenAi | ProviderKind::OpenRouter | ProviderKind::Local
                ),
                "the OpenAI Responses API",
            ),
        };
        match speaks {
            true => Ok(p),
            false => Err(format!("provider {} does not speak {name}", p.id)),
        }
    }
}

/// The gateway's routes, each answered by `serve` with its route and its body unread (a call is
/// let through before its body is read): the control box's ([`router`]) and a worker's
/// ([`worker_router`]).
fn routes<F, Fut>(serve: F) -> Router<Arc<AppState>>
where
    F: Fn(Arc<AppState>, Route, HeaderMap, Body) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    let mut router = Router::new();
    for (path, api) in ROUTES {
        let serve = serve.clone();
        let handler = move |State(state): State<Arc<AppState>>,
                            Path((branch, agent)): Path<(String, String)>,
                            headers: HeaderMap,
                            body: Body| {
            let route = Route { branch, agent, api };
            serve(state, route, headers, body)
        };
        router = router.route(path, post(handler));
    }
    router
}

pub fn router() -> Router<Arc<AppState>> {
    routes(serve_call)
}

/// The control box's answer to a call on `route`.
async fn serve_call(
    state: Arc<AppState>,
    route: Route,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let route = match world::is_world(&route) {
        true => match world::routed(route, &headers) {
            Ok(r) => r,
            Err(refused) => return *refused,
        },
        false => route,
    };
    match route.api {
        Api::Chat => chat::chat(state, route, headers, body).await,
        Api::Messages | Api::Responses => passthrough::pass(state, route, headers, body).await,
        Api::CountTokens => passthrough::count(state, route, headers, body).await,
    }
}

fn error_response(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(json!({"error": msg.into()}))).into_response()
}

/// Why a call is turned away before it reaches a provider: its status and message.
type Refused = (StatusCode, &'static str);

/// What stands in for the message of an answer that turned the key down.
pub const REJECTED_MESSAGE: &str = "(not recorded: the provider rejected the key)";

/// `body` of an answer with `status`, without its error message when it turned the key down
/// (401 or 403: the message may quote the key, masked); the error's type and code stay.
fn redact(status: u16, body: &mut Value) {
    if !matches!(status, 401 | 403) {
        return;
    }
    match body.get_mut("error") {
        Some(Value::Object(e)) if e.contains_key("message") => {
            e.insert("message".into(), json!(REJECTED_MESSAGE));
        }
        Some(e @ Value::String(_)) => *e = json!(REJECTED_MESSAGE),
        _ => {}
    }
    // An answer that wasn't JSON is kept as `{"raw": text}`.
    if let Some(raw) = body.get_mut("raw") {
        *raw = json!(REJECTED_MESSAGE);
    }
}

/// `status` and `body`, with `retry-after` seconds when given.
fn answer(status: u16, body: Value, retry_after: Option<u64>) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut response = (status, Json(body)).into_response();
    if let Some(secs) = retry_after {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, secs.into());
    }
    response
}

/// `response` saying what the call cost ([`COST_HEADER`]), when that is known.
fn with_cost(mut response: Response, cost: Option<f64>) -> Response {
    let value = cost.and_then(|usd| HeaderValue::from_str(&usd.to_string()).ok());
    if let Some(v) = value {
        response.headers_mut().insert(COST_HEADER, v);
    }
    response
}
