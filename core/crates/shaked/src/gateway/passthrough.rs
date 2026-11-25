//! `POST /gateway/{branch}/{agent}/v1/messages` (Anthropic Messages) and `…/v1/responses`
//! (OpenAI Responses): what a CLI agent (Claude Code, Codex) calls, in a sandbox through its
//! worker. The request goes to the agent's provider as it came, with the key `chat/completions`
//! would choose for the branch's caller; of the caller's headers only the API's version and
//! betas go along, never its token. The answer comes back as it came, a stream chunk by chunk,
//! and the call is recorded once, raw, when it ends ([`super::raw_call`]).
//! `…/v1/messages/count_tokens` goes the same way, but is neither recorded nor charged: it
//! counts, and costs nothing.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde_json::json;

use super::{admit, answer, error_response, raw_call::Call, redact, target::Route, Api};
use crate::app::AppState;

/// The caller's headers that go on to the provider: the API's version and betas.
pub(super) const API_HEADERS: [&str; 3] = ["anthropic-version", "anthropic-beta", "openai-beta"];

/// Of `headers`, only [`API_HEADERS`].
pub(super) fn api_headers(headers: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for name in API_HEADERS {
        for v in headers.get_all(name) {
            out.append(name, v.clone());
        }
    }
    out
}

/// A Messages or Responses call, admitted and sent on.
pub(super) async fn pass(
    state: Arc<AppState>,
    route: Route,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let prepared = match admit::prepared(&state, &headers, &route, body).await {
        Ok(a) => a,
        Err(refused) => return *refused,
    };
    let call = Call {
        state,
        route,
        prepared,
    };
    call.send(&headers).await
}

/// A `count_tokens` call: checked and held as a model call is, sent on and answered, neither
/// recorded nor charged (a rejected key's message left out).
pub(super) async fn count(
    state: Arc<AppState>,
    route: Route,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let a = match admit::prepared(&state, &headers, &route, body).await {
        Ok(a) => a,
        Err(refused) => return *refused,
    };
    let req = a.provider.raw_post(&state.http, Api::CountTokens.path());
    let sent = req
        .headers(api_headers(&headers))
        .json(&a.request)
        .send()
        .await;
    let Ok(r) = sent else {
        return error_response(StatusCode::BAD_GATEWAY, "upstream request failed");
    };
    let status = r.status().as_u16();
    let mut body = r.json().await.unwrap_or_else(|_| json!({}));
    redact(status, &mut body);
    answer(status, body, None)
}
