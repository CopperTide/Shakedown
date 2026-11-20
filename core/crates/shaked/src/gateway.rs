use std::{sync::Arc, time::Instant};

use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};

use crate::{
    app::AppState,
    exec::BranchCtx,
    layer::{Costly, KeySource, Meter, ModelWant, Usage},
    model::*,
    overhead::{ms_since, Cat},
    providers::{Provider, ProviderKind, Upstream},
    reasoning,
    store::Store,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route(
        "/gateway/{branch}/{agent}/v1/chat/completions",
        post(handle),
    )
}

fn error_response(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(json!({"error": msg.into()}))).into_response()
}

/// What the agent's provider answered: `Err(message)` when it can't be reached (or isn't known).
async fn send_upstream(
    state: &AppState,
    provider: &Result<Provider, String>,
    body: &Value,
) -> Result<Upstream, String> {
    match provider {
        Ok(p) => p.send(&state.http, body).await.map(redact_rejection),
        Err(e) => Err(e.clone()),
    }
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
    provider: &Result<Provider, String>,
    upstream: &Result<Upstream, String>,
) -> (u16, ModelCallPayload) {
    let (status, response, ok) = answer_for(upstream);
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    let known = provider.as_ref().ok().filter(|_| ok);
    let cost_usd = known.and_then(|p| p.cost(&usage));
    let payload = ModelCallPayload {
        request,
        usage,
        cost_source: cost_usd
            .and(known.and_then(Provider::cost_source))
            .map(str::to_string),
        cost_usd,
        provider: provider.as_ref().ok().map(|p| p.id.clone()),
        provider_response: raw_evidence(upstream),
        response,
    };
    (status, payload)
}

/// Adds the call to the run's spend estimate; the loop that made the call checks the run's limit
/// once it has the answer.
fn charge(ctx: &BranchCtx, agent: &str, provider: &Result<Provider, String>, p: &ModelCallPayload) {
    if let Some(usd) = call_cost(provider, p) {
        let _ = ctx.budget.add(agent, usd);
    }
}

/// What a call cost: the provider's figure, else its list price for the model it asked for
/// (the OpenAI-shape usage, cached prompt tokens priced as cache hits).
fn call_cost(provider: &Result<Provider, String>, p: &ModelCallPayload) -> Option<f64> {
    if p.cost_usd.is_some() {
        return p.cost_usd;
    }
    let n = |v: &Value| v.as_i64().unwrap_or(0);
    let u = &p.usage;
    let cached = n(&u["prompt_tokens_details"]["cached_tokens"]);
    let t = crate::prices::CallTokens {
        input: (n(&u["prompt_tokens"]) - cached).max(0),
        cache_read: cached,
        output: n(&u["completion_tokens"]),
        ..Default::default()
    };
    let id = provider.as_ref().ok()?.id.as_str();
    crate::budget::price(id, p.request["model"].as_str(), &t)
}

async fn record(
    ctx: &BranchCtx,
    store: &Store,
    agent: &str,
    payload: &ModelCallPayload,
) -> Result<i64, String> {
    let v = serde_json::to_value(payload).map_err(|e| e.to_string())?;
    let ev = ctx
        .append(store, Some(agent), EventKind::ModelCall, v)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let items =
        reasoning::from_chat_response(&payload.response, payload.provider_response.as_ref());
    reasoning::record(
        ctx,
        store,
        agent,
        items,
        reasoning::SOURCE_GATEWAY,
        Some(ev.seq),
    )
    .await;
    Ok(ev.seq)
}

/// Why a call is turned away before it reaches a provider: its status and message.
type Refused = (StatusCode, &'static str);

/// The live `branch` and the provider of its `agent` (`None`: the default one): 404 when the
/// branch isn't live or has no such agent, 401 when the agent was given a token (as its MCP
/// endpoint wants) and the call doesn't carry it.
fn live_agent(
    state: &AppState,
    branch: &str,
    agent: &str,
    headers: &HeaderMap,
) -> Result<(Arc<BranchCtx>, Option<String>), Refused> {
    let unknown = (StatusCode::NOT_FOUND, "unknown branch or agent");
    let ctx = state.live.get(branch).ok_or(unknown)?;
    let provider = ctx.roster.provider_of(agent).ok_or(unknown)?;
    let authorization = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !ctx.roster.token_ok(agent, authorization) {
        let why = "this endpoint wants the run's bearer token";
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
async fn held_back(state: &AppState, ctx: &BranchCtx) -> Option<Response> {
    if ctx.flow.go().await.is_err() {
        return Some(error_response(StatusCode::CONFLICT, crate::flow::STOPPED));
    }
    let why = over_the_limit(state, ctx).await?;
    Some(error_response(StatusCode::PAYMENT_REQUIRED, why))
}

/// Why a run an external system drives may make no more model calls: its spend estimate passed
/// its limit (shaked's own runners end such a run themselves). The first refusal is recorded as
/// `budget/limit_reached`; nothing is sent upstream.
async fn over_the_limit(state: &AppState, ctx: &BranchCtx) -> Option<String> {
    if ctx.rules.runner != Runner::External {
        return None;
    }
    let why = ctx.budget.over()?;
    if ctx.budget.first_refusal() {
        let ev = json!({"part": "budget", "kind": "limit_reached", "text": why, "data": null,
                        "clock_ms": ctx.clock.now_ms()});
        let recorded = ctx.append_now(&state.store, None, EventKind::WorldEvent, ev);
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
    provider: &Result<Provider, String>,
    body: &Value,
) -> (Result<Upstream, String>, f64) {
    let asked = Instant::now();
    let upstream = send_upstream(state, provider, body).await;
    (upstream, ms_since(asked))
}

async fn handle(
    State(state): State<Arc<AppState>>,
    Path((branch, agent)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let (ctx, provider) = match live_agent(&state, &branch, &agent, &headers) {
        Ok(found) => found,
        Err((status, why)) => return error_response(status, why),
    };
    if let Some(refused) = held_back(&state, &ctx).await {
        return refused;
    }
    save_before_model_call(&state, &ctx).await;
    let since = Instant::now();
    if !body.is_object() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "request body must be a JSON object",
        );
    }
    let model = body["model"].as_str().unwrap_or_default();
    let asked = provider.as_deref().unwrap_or_default();
    let op = Costly::ModelCall {
        branch: &branch,
        provider: asked,
        model,
    };
    if let Err(r) = crate::layer::admit(&state.store, &ctx.caller, op).await {
        return error_response(r.status, r.message);
    }
    call(&state, &ctx, &agent, body, provider.as_deref(), since).await
}

/// The call itself, once admitted: to the provider, recorded, charged and metered.
async fn call(
    state: &AppState,
    ctx: &BranchCtx,
    agent: &str,
    body: Value,
    asked: Option<&str>,
    since: Instant,
) -> Response {
    let (provider, paid_by) = provider_for(state, ctx, asked, &body).await;
    ctx.roster.note_model_call(agent);
    let (upstream, upstream_ms) = timed_upstream(state, &provider, &body).await;
    let (status, payload) = model_call_payload(body, &provider, &upstream);
    // The gateway's own handling: all but the provider's answer and the time recording takes.
    ctx.overhead.add_besides(Cat::Gateway, since, upstream_ms);
    charge(ctx, agent, &provider, &payload);
    let seq = match record(ctx, &state.store, agent, &payload).await {
        Ok(seq) => seq,
        Err(e) => {
            let why = format!("recording model call: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, why);
        }
    };
    meter_call(state, ctx, &provider, &payload, seq, paid_by).await;
    reply(status, payload.response, &upstream)
}

/// The provider `asked` names and whose key pays: the layer's fallback when that provider needs a
/// key the environment does not have.
async fn provider_for(
    state: &AppState,
    ctx: &BranchCtx,
    asked: Option<&str>,
    body: &Value,
) -> (Result<Provider, String>, KeySource) {
    let found = state.config.providers.resolve(asked);
    let keyed = [
        ProviderKind::OpenRouter,
        ProviderKind::OpenAi,
        ProviderKind::Anthropic,
    ];
    let keyless = matches!(&found, Ok(p) if keyed.contains(&p.kind) && p.endpoint.key.is_none());
    if keyless {
        let model = body["model"].as_str().unwrap_or_default();
        let want = ModelWant {
            provider: asked,
            model,
        };
        if let Some(f) = state
            .store
            .layer()
            .fallback_provider(&ctx.caller, want)
            .await
        {
            return (Ok(f.provider), KeySource::Fallback(f.source));
        }
    }
    (found, KeySource::Environment)
}

/// The layer's meters for one recorded call: its cost in micro-USD and its tokens.
async fn meter_call(
    state: &AppState,
    ctx: &BranchCtx,
    provider: &Result<Provider, String>,
    p: &ModelCallPayload,
    seq: i64,
    paid_by: KeySource,
) {
    let key = format!("gateway:{}:{seq}", ctx.branch_id);
    let micros = (call_cost(provider, p).unwrap_or(0.0) * 1e6).round() as i64;
    let count = |k: &str| p.usage[k].as_i64().unwrap_or(0);
    let tokens = p.usage["total_tokens"]
        .as_i64()
        .unwrap_or_else(|| count("prompt_tokens") + count("completion_tokens"));
    let usage = |meter, quantity, key| Usage {
        meter,
        quantity,
        key,
        run: None,
        branch: Some(&ctx.branch_id),
        paid_by,
    };
    let store = &state.store;
    crate::layer::meter(
        store,
        &ctx.caller,
        usage(Meter::ModelUsdMicros, micros, key.clone()),
    )
    .await;
    crate::layer::meter(
        store,
        &ctx.caller,
        usage(Meter::ModelTokens, tokens, format!("{key}:tokens")),
    )
    .await;
}

/// The agent's answer: `status` and `body`, with the provider's `retry-after` when it failed.
fn reply(status: u16, body: Value, upstream: &Result<Upstream, String>) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut response = (status, Json(body)).into_response();
    if let Some(secs) = retry_after(upstream) {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, secs.into());
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
