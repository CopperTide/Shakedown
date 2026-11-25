//! `POST /gateway/{branch}/{agent}/v1/chat/completions`: the canonical format shaked's own loop
//! speaks, sent to any provider through its adapter ([`Provider::send`]), answered whole, and
//! recorded as a canonical `model_call` with its reasoning, the provider's own answer kept
//! beside it when it differs.

use std::{sync::Arc, time::Instant};

use axum::{body::Body, http::HeaderMap, http::StatusCode, response::Response};
use serde_json::{json, Value};

use super::{admit, answer, book::Outcome, error_response, redact, target::Route, with_cost};
use crate::{
    app::AppState,
    model::*,
    overhead::ms_since,
    providers::{Provider, Upstream},
    reasoning,
};

/// A `chat/completions` call, admitted: to the provider, recorded, charged and metered.
pub(super) async fn chat(
    state: Arc<AppState>,
    route: Route,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let a = match admit::prepared(&state, &headers, &route, body).await {
        Ok(a) => a,
        Err(refused) => return *refused,
    };
    let (target, agent) = (&a.target, &route.agent);
    let (upstream, upstream_ms) = upstream(&state, &a.provider, &a.request).await;
    let (status, mut payload) = model_call_payload(a.request, &a.provider, &upstream);
    payload.duration_ms = Some(upstream_ms);
    target.overhead(a.since, upstream_ms);
    let (charge, v) = target.bill(agent, (&a.provider, &payload), Outcome::of(status, None));
    let items =
        reasoning::from_chat_response(&payload.response, payload.provider_response.as_ref());
    let metered = (charge.usd, &payload.usage, a.paid_by);
    let booked = target.book(&state, agent, (v, items), metered).await;
    // Charged either way: a call that put the run over its limit stops it now, recorded or not.
    target.settle(&state).await;
    if let Err(e) = booked {
        let why = format!("recording model call: {e}");
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, why);
    }
    let reply = answer(status, payload.response, retry_after(&upstream));
    with_cost(reply, charge.usd)
}

/// What the provider answered to `body` (`Err(message)` when it can't be reached; a rejected
/// key's message left out), and how long it took (ms).
async fn upstream(state: &AppState, p: &Provider, body: &Value) -> (Result<Upstream, String>, f64) {
    let asked = Instant::now();
    let upstream = p.send(&state.http, body).await.map(redact_rejection);
    (upstream, ms_since(asked))
}

/// A 401 or 403 answer without its error message, in its canonical and its own form.
fn redact_rejection(mut u: Upstream) -> Upstream {
    redact(u.status, &mut u.response);
    redact(u.status, &mut u.raw);
    u
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
        duration_ms: None,
    };
    (status, payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::REJECTED_MESSAGE;

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
