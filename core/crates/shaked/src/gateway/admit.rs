//! What lets a call through to its provider, the same way on every route: whom it is for and
//! that its branch does not hold it back ([`let_through`]), and only then its body, read up to
//! its API's limit; then the layer's admission of a model call, and its provider with the key
//! chosen for the branch's caller ([`prepared`]).

use std::{sync::Arc, time::Instant};

use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use http_body_util::LengthLimitError;
use serde_json::{json, Value};

use super::{
    error_response,
    raw_call::marked,
    target::{self, CallFor, Kind, Route},
    Api,
};
use crate::{
    app::AppState,
    layer::{Costly, KeySource},
    model::*,
    providers::{Provider, ProviderKind},
};

/// The call on `route`, let through to its body: whom it is for, not held back by its branch,
/// and (a model call) saved before and noted. Its body is read only then: a call nobody may make
/// never has it read.
pub(super) async fn let_through(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    route: &Route,
) -> Result<(CallFor, Option<String>), Box<Response>> {
    let (target, provider) = target::resolve(state, headers, route).await?;
    if let Some(refused) = target.held_back(state).await {
        return Err(Box::new(refused));
    }
    if route.kind() == Kind::ModelCall {
        target.save_point(state).await;
        target.note_model_call(&route.agent);
    }
    Ok((target, provider))
}

/// The body of a call on `route`: 413 past its API's limit, 400 when it could not be read (the
/// caller went away, or its upload broke off).
pub(super) async fn read_body(body: Body, route: &Route) -> Result<Bytes, Box<Response>> {
    let limit = route.api.max_body();
    let read = axum::body::to_bytes(body, limit).await;
    read.map_err(|e| {
        let refused = match e.into_inner().is::<LengthLimitError>() {
            true => (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("request body over {} MiB", limit >> 20),
            ),
            false => (StatusCode::BAD_REQUEST, BODY_UNREADABLE.to_string()),
        };
        Box::new(error_response(refused.0, refused.1))
    })
}

/// What a call whose body could not be read is told (400).
const BODY_UNREADABLE: &str = "request body could not be read";

/// A call ready to go to its provider: for whom, since when (its body read), the provider, whose
/// key pays, and the call as it is sent.
pub(super) struct Prepared {
    pub target: CallFor,
    pub since: Instant,
    pub provider: Provider,
    pub paid_by: KeySource,
    pub request: Value,
}

/// The call on `route` [`let_through`], its body a JSON object, (a model call) admitted by the
/// layer, and its provider found ([`provider`]). The gateway's own time on it starts once its
/// body is read: the caller's upload is none of it.
pub(super) async fn prepared(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    route: &Route,
    body: Body,
) -> Result<Prepared, Box<Response>> {
    let (target, provider) = let_through(state, headers, route).await?;
    let bytes = read_body(body, route).await?;
    let since = Instant::now();
    let request = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object);
    let why = "request body must be a JSON object";
    let request = request.ok_or_else(|| error_response(StatusCode::BAD_REQUEST, why))?;
    let asked = provider.or_else(|| route.api.default_provider().map(str::to_string));
    if route.kind() == Kind::ModelCall {
        layer_admits(state, &target, route, (asked.as_deref(), &request)).await?;
    }
    let (provider, paid_by, request) =
        self::provider(state, &target, route, asked.as_deref(), request).await?;
    Ok(Prepared {
        target,
        since,
        provider,
        paid_by,
        request,
    })
}

/// Ok when the layer admits the model call `request` to the provider `asked` (its quotas).
async fn layer_admits(
    state: &AppState,
    target: &CallFor,
    route: &Route,
    (asked, request): (Option<&str>, &Value),
) -> Result<(), Box<Response>> {
    let op = costly_call(&route.branch, asked.unwrap_or_default(), request);
    let admitted = crate::layer::admit(&state.store, target.caller(), op).await;
    admitted
        .map(drop)
        .map_err(|r| Box::new(error_response(r.status, r.message)))
}

/// What the layer is asked before a call of `body` on `branch` to the provider `asked`.
fn costly_call<'a>(branch: &'a str, asked: &'a str, body: &'a Value) -> Costly<'a> {
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

/// The provider `asked` names for `request` on `route`, with the key chosen for the branch's
/// caller: 502 when it can't be found (a model call is recorded as failed), 400 when it doesn't
/// speak the route's API or (on a linked branch) its cost could not be known. A model call is held
/// to a lent provider's cap, and OpenRouter is asked to report its cost.
async fn provider(
    state: &AppState,
    target: &CallFor,
    route: &Route,
    asked: Option<&str>,
    request: Value,
) -> Result<(Provider, KeySource, Value), Box<Response>> {
    let model = request["model"].as_str().unwrap_or_default().to_string();
    let keys = crate::providers::keys::provider_for(state, target.caller(), asked, &model);
    let (found, paid_by) = keys.await;
    let found = match found {
        Ok(p) => p,
        Err(why) => return Err(unreached(state, target, route, request, why).await),
    };
    let refused = |why| Box::new(error_response(StatusCode::BAD_REQUEST, why));
    let provider = route.api.speaking(found).map_err(refused)?;
    if let Some(why) = target.unpriced(&provider, &model) {
        return Err(refused(why));
    }
    let request = match route.kind() {
        Kind::ModelCall => capped(route.api, &provider, request),
        Kind::TokenCount => request,
    };
    Ok((provider, paid_by, request))
}

/// `request` as it goes to `p`: OpenRouter's Responses are asked to report their cost (its chat
/// adapter asks itself), and a lent provider's limits (its `cap`) hold: `max_tokens`, or
/// Responses' `max_output_tokens`.
fn capped(api: Api, p: &Provider, mut request: Value) -> Value {
    if p.kind == ProviderKind::OpenRouter && api == Api::Responses {
        request["usage"] = json!({"include": true});
    }
    let Some(cap) = p.cap else {
        return request;
    };
    if api != Api::Responses {
        return cap.apply(p.kind, request);
    }
    let asked = request["max_output_tokens"].as_u64();
    request["max_output_tokens"] = json!(asked.map_or(cap.max_tokens, |n| n.min(cap.max_tokens)));
    request
}

/// The 502 of a call whose provider could not be found (`why`); a model call is recorded as a
/// failed call, as one the provider turned down is.
async fn unreached(
    state: &AppState,
    target: &CallFor,
    route: &Route,
    request: Value,
    why: String,
) -> Box<Response> {
    if route.kind() == Kind::ModelCall {
        let payload = ModelCallPayload {
            request,
            response: json!({ "error": why }),
            usage: json!({}),
            cost_usd: None,
            cost_source: None,
            provider: None,
            provider_response: None,
        };
        let mut v = serde_json::to_value(&payload).unwrap_or_default();
        marked(&mut v, route.api, None);
        let recorded = target.append(state, &route.agent, EventKind::ModelCall, v);
        if let Err(e) = recorded.await {
            eprintln!("gateway: recording a model call on {}: {e:#}", route.branch);
        }
    }
    Box::new(error_response(StatusCode::BAD_GATEWAY, why))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages() -> Route {
        Route {
            branch: "b".into(),
            agent: "a".into(),
            api: Api::Messages,
        }
    }

    async fn refused(body: Body) -> (StatusCode, Value) {
        let r = *read_body(body, &messages()).await.unwrap_err();
        let status = r.status();
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await;
        (status, serde_json::from_slice(&bytes.unwrap()).unwrap())
    }

    #[tokio::test]
    async fn a_body_past_its_apis_limit_is_413() {
        let (status, v) = refused(Body::from(vec![b' '; (32 << 20) + 1])).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(v["error"], "request body over 32 MiB");
    }

    /// The caller went away mid-upload, or its upload broke: 400, not 413.
    #[tokio::test]
    async fn a_body_that_cannot_be_read_is_400() {
        let chunks = [
            Ok(Bytes::from_static(b"{")),
            Err(std::io::Error::other("gone")),
        ];
        let (status, v) = refused(Body::from_stream(futures_util::stream::iter(chunks))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(v["error"], BODY_UNREADABLE);
    }
}
