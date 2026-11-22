//! The recording model gateway, `/gateway/{branch}/{agent}/v1/…`: `chat/completions` (the
//! canonical format shaked's own loop speaks, sent to any provider), and the pass-through routes
//! `messages` (Anthropic Messages) and `responses` (OpenAI Responses) for CLI agents
//! ([`passthrough`]). Each takes the agent's token, or on the control box its branch's worker
//! token ([`target`]); the provider gets the key chosen for the branch's caller, never the
//! caller's own. In a `shaked worker` every route goes on to the control box ([`forward`]).

mod charge;
mod forward;
mod linked;
mod passthrough;
mod raw_call;
mod sse;
mod target;

use std::{future::Future, sync::Arc, time::Instant};

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
use target::{CallFor, Outcome, Route};

use crate::{
    app::AppState,
    exec::BranchCtx,
    layer::{Costly, KeySource, Meter, Usage},
    model::*,
    overhead::ms_since,
    providers::{Provider, ProviderKind, Upstream, ANTHROPIC, OPENAI},
    reasoning,
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
    match route.api {
        Api::Chat => chat(state, route, headers, body).await,
        Api::Messages | Api::Responses => passthrough::pass(state, route, headers, body).await,
        Api::CountTokens => passthrough::count(state, route, headers, body).await,
    }
}

fn error_response(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(json!({"error": msg.into()}))).into_response()
}

/// What the agent's provider answered: `Err(message)` when it can't be reached.
async fn send_upstream(state: &AppState, p: &Provider, body: &Value) -> Result<Upstream, String> {
    p.send(&state.http, body).await.map(redact_rejection)
}

/// What stands in for the message of an answer that turned the key down.
pub const REJECTED_MESSAGE: &str = "(not recorded: the provider rejected the key)";

/// A 401 or 403 answer without its error message, which may quote the key (masked); the
/// error's type and code stay.
fn redact_rejection(mut u: Upstream) -> Upstream {
    if matches!(u.status, 401 | 403) {
        redact_message(&mut u.response);
        redact_message(&mut u.raw);
    }
    u
}

fn redact_message(body: &mut Value) {
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

/// The status to answer with, the response body, and whether the call succeeded. A provider's
/// error status is passed on (so the agent loop can tell overload and rate limits); no answer,
/// or an answer that is neither success nor error, is a 502.
fn answer_for(upstream: &Result<Upstream, String>) -> (u16, Value, bool) {
    match upstream {
        Err(msg) => (502, json!({"error": msg}), false),
        Ok(u) if (200..300).contains(&u.status) => (u.status, u.response.clone(), true),
        Ok(u) => (
            if (400..600).contains(&u.status) {
                u.status
            } else {
                502
            },
            json!({"error": format!("upstream returned {}", u.status), "status": u.status, "body": u.response}),
            false,
        ),
    }
}

/// The `retry-after` to pass on: what a provider that failed asked for.
fn retry_after(upstream: &Result<Upstream, String>) -> Option<u64> {
    upstream
        .as_ref()
        .ok()
        .filter(|u| !(200..300).contains(&u.status))
        .and_then(|u| u.retry_after)
}

/// The provider's own response, kept when it isn't the canonical one already recorded.
fn raw_evidence(upstream: &Result<Upstream, String>) -> Option<Value> {
    upstream
        .as_ref()
        .ok()
        .filter(|u| u.raw != u.response)
        .map(|u| u.raw.clone())
}

/// The event of one model call: the canonical request and response, the provider's own
/// response when it differs, and the cost the provider makes known.
fn model_call_payload(
    request: Value,
    provider: &Provider,
    upstream: &Result<Upstream, String>,
) -> (u16, ModelCallPayload) {
    let (status, response, ok) = answer_for(upstream);
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    let cost_usd = ok.then(|| provider.cost(&usage)).flatten();
    let payload = ModelCallPayload {
        request,
        usage,
        cost_source: cost_usd.and(provider.cost_source()).map(str::to_string),
        cost_usd,
        provider: Some(provider.id.clone()),
        provider_response: raw_evidence(upstream),
        response,
    };
    (status, payload)
}

/// What a call cost: the provider's figure, else its list price for the model it asked for
/// (cached prompt tokens priced as cache hits, cache writes as writes).
fn call_cost(provider: &Provider, p: &ModelCallPayload) -> Option<f64> {
    if p.cost_usd.is_some() {
        return p.cost_usd;
    }
    let n = |v: &Value| v.as_i64().unwrap_or(0);
    let u = &p.usage;
    let cached =
        n(&u["prompt_tokens_details"]["cached_tokens"]).max(n(&u["cache_read_input_tokens"]));
    let written = n(&u["cache_creation_input_tokens"]);
    let t = crate::prices::CallTokens {
        input: (n(&u["prompt_tokens"]) - cached - written).max(0),
        cache_read: cached,
        cache_write: written,
        output: n(&u["completion_tokens"]),
    };
    crate::budget::price(&provider.id, p.request["model"].as_str(), &t)
}

/// Records the call `payload` as its event `v`, and its reasoning: its seq.
async fn record(
    state: &AppState,
    target: &CallFor,
    agent: &str,
    (payload, v): (&ModelCallPayload, Value),
) -> Result<i64, String> {
    let ev = target.append(state, agent, EventKind::ModelCall, v).await;
    let ev = ev.map_err(|e| format!("{e:#}"))?;
    let items =
        reasoning::from_chat_response(&payload.response, payload.provider_response.as_ref());
    target.reasoning(state, agent, items, ev.seq).await;
    Ok(ev.seq)
}

/// Why a call is turned away before it reaches a provider: its status and message.
type Refused = (StatusCode, &'static str);

/// The live `branch` and the provider of its `agent` (`None`: the default one): 404 when the
/// branch isn't live or has no such agent, 401 when the call doesn't carry the agent's token
/// (as a bearer token, or as `x-api-key` the way Claude Code sends its key).
fn live_agent(
    state: &AppState,
    branch: &str,
    agent: &str,
    headers: &HeaderMap,
) -> Result<(Arc<BranchCtx>, Option<String>), Refused> {
    let unknown = (StatusCode::NOT_FOUND, "unknown branch or agent");
    let ctx = state.live.get(branch).ok_or(unknown)?;
    let provider = ctx.roster.provider_of(agent).ok_or(unknown)?;
    let given = target::credential(headers).map(|t| format!("Bearer {t}"));
    if !ctx.roster.token_ok(agent, given.as_deref()) {
        let why = crate::external::TOKEN_WANTED;
        return Err((StatusCode::UNAUTHORIZED, why));
    }
    Ok((ctx, provider))
}

/// A save point before a model call, when a part that saves at points changed: the world is then
/// restorable where a fork at this call starts. A failure is logged; the call goes on.
async fn save_before_model_call(state: &AppState, ctx: &BranchCtx) {
    let exec = crate::exec::ToolExec::new(state.store.clone());
    if let Err(e) = exec.save_point(ctx, SAVE_BEFORE_MODEL_CALL).await {
        eprintln!("gateway: save point on {}: {e}", ctx.branch_id);
    }
}

/// Why the call can't go on once the branch lets it: 409 when the branch stopped (a paused one
/// holds the call until it resumes, a wait that is no part of the gateway's), 402 past the spend
/// limit of a run an external system drives.
async fn held_back(ctx: &BranchCtx) -> Option<Response> {
    if ctx.flow.go().await.is_err() {
        return Some(error_response(StatusCode::CONFLICT, crate::flow::STOPPED));
    }
    let why = over_the_limit(ctx).await?;
    Some(error_response(StatusCode::PAYMENT_REQUIRED, why))
}

/// Why a run an external system drives may make no more model calls: its spend estimate passed
/// its limit (shaked's own runners end such a run themselves). The first refusal is recorded as
/// `budget/limit_reached`; nothing is sent upstream.
async fn over_the_limit(ctx: &BranchCtx) -> Option<String> {
    if ctx.rules.runner != Runner::External {
        return None;
    }
    let why = ctx.budget.over()?;
    if ctx.budget.first_refusal() {
        let ev = json!({"part": "budget", "kind": "limit_reached", "text": why, "data": null,
                        "clock_ms": ctx.clock.now_ms()});
        let recorded = ctx.append_now(None, EventKind::WorldEvent, ev);
        if let Err(e) = recorded.await {
            eprintln!(
                "gateway: recording the spend limit on {}: {e:#}",
                ctx.branch_id
            );
        }
    }
    Some(why)
}

/// The provider's answer to `body`, and how long it took (ms).
async fn timed_upstream(
    state: &AppState,
    provider: &Provider,
    body: &Value,
) -> (Result<Upstream, String>, f64) {
    let asked = Instant::now();
    let upstream = send_upstream(state, provider, body).await;
    (upstream, ms_since(asked))
}

/// What the layer is asked before a call of `body` on `branch` to the provider `asked`.
fn model_call<'a>(branch: &'a str, asked: &'a str, body: &'a Value) -> Costly<'a> {
    let max_tokens = ["max_tokens", "max_completion_tokens"]
        .iter()
        .find_map(|f| body[*f].as_u64());
    Costly::ModelCall {
        branch,
        provider: asked,
        model: body["model"].as_str().unwrap_or_default(),
        max_tokens,
        request_bytes: body.to_string().len() as u64,
    }
}

/// A `chat/completions` call, admitted: to the provider, recorded, charged and metered.
async fn chat(state: Arc<AppState>, route: Route, headers: HeaderMap, body: Body) -> Response {
    let a = match target::prepared(&state, &headers, &route, body).await {
        Ok(a) => a,
        Err(refused) => return *refused,
    };
    let (target, agent) = (&a.target, &route.agent);
    let (upstream, upstream_ms) = timed_upstream(&state, &a.provider, &a.request).await;
    let (status, payload) = model_call_payload(a.request, &a.provider, &upstream);
    target.overhead(a.since, upstream_ms);
    let outcome = Outcome::of(status, None);
    let (charge, v) = target.bill(agent, (&a.provider, &payload), outcome);
    let seq = match record(&state, target, agent, (&payload, v)).await {
        Ok(seq) => seq,
        Err(e) => {
            let why = format!("recording model call: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, why);
        }
    };
    meter_call(&state, target, charge.usd, &payload.usage, (seq, a.paid_by)).await;
    target.settle(&state).await;
    with_cost(reply(status, payload.response, &upstream), charge.usd)
}

/// The layer's meters for one recorded call (its seq, and whose key paid): its cost in
/// micro-USD and its tokens.
async fn meter_call(
    state: &AppState,
    target: &CallFor,
    cost: Option<f64>,
    usage: &Value,
    (seq, paid_by): (i64, KeySource),
) {
    let branch = target.branch_id();
    let key = format!("gateway:{branch}:{seq}");
    let micros = (cost.unwrap_or(0.0) * 1e6).round() as i64;
    let count = |k: &str| usage[k].as_i64().unwrap_or(0);
    let tokens = usage["total_tokens"]
        .as_i64()
        .unwrap_or_else(|| count("prompt_tokens") + count("completion_tokens"));
    let usage = |meter, quantity, key| Usage {
        meter,
        quantity,
        key,
        run: None,
        branch: Some(branch),
        paid_by,
    };
    let (store, caller) = (&state.store, target.caller());
    let cost = usage(Meter::ModelUsdMicros, micros, key.clone());
    crate::layer::meter(store, caller, cost).await;
    let tokens = usage(Meter::ModelTokens, tokens, format!("{key}:tokens"));
    crate::layer::meter(store, caller, tokens).await;
}

/// The agent's answer: `status` and `body`, with the provider's `retry-after` when it failed.
fn reply(status: u16, body: Value, upstream: &Result<Upstream, String>) -> Response {
    answer(status, body, retry_after(upstream))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(status: u16, body: Value) -> Upstream {
        redact_rejection(Upstream {
            status,
            response: body.clone(),
            raw: body,
            retry_after: None,
        })
    }

    #[test]
    fn a_rejected_keys_message_is_not_kept() {
        let openai = json!({"error": {"message": "Incorrect API key provided: sk-ab***yz",
                                      "type": "invalid_request_error", "code": "invalid_api_key"}});
        let u = answer(401, openai);
        let want = json!({"error": {"message": REJECTED_MESSAGE,
                                    "type": "invalid_request_error", "code": "invalid_api_key"}});
        assert_eq!((&u.response, &u.raw), (&want, &want));
        let claude = json!({"type": "error",
                            "error": {"type": "permission_error", "message": "key sk-ant-***"}});
        let u = answer(403, claude);
        assert_eq!(u.raw["error"]["type"], "permission_error");
        assert_eq!(u.raw["error"]["message"], REJECTED_MESSAGE);
        assert_eq!(
            answer(401, json!({"raw": "bad key sk-x"}).clone()).raw["raw"],
            REJECTED_MESSAGE
        );
        assert_eq!(
            answer(401, json!({"error": "bad key sk-x"})).response["error"],
            REJECTED_MESSAGE
        );
        // Other failures keep their message.
        let busy = json!({"error": {"message": "overloaded"}});
        assert_eq!(answer(529, busy.clone()).raw, busy);
    }
}
