//! The `/api` auth layer: inbound gateway headers are dropped (unless the layer serves others: it
//! verifies them itself), a Bearer token is resolved by the engine (an unknown, revoked or expired
//! one is 401), and the layer's `authenticate` decides who calls. `GET /api/health` needs no caller.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use super::{run_tokens::resolve_run_token, tokens::resolve_api_token, Caller};
use crate::{app::AppState, store::Store};

/// Headers a hosting gateway sets; never trusted from a caller here. A layer that serves others
/// receives them, since it is the one that checks the gateway's signature.
const GATEWAY_HEADER_PREFIX: &str = "x-sculptured-";

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error": message}))).into_response()
}

/// The caller a Bearer secret names: `Ok(None)` when no token matches.
async fn resolve(store: &Store, bearer: &str) -> anyhow::Result<Option<Caller>> {
    match bearer.starts_with(super::tokens::API_PREFIX) {
        true => resolve_api_token(store, bearer).await,
        false => resolve_run_token(store, bearer).await,
    }
}

/// The answer to an Authorization header that carries no Bearer token.
const BEARER_WANTED: &str = "send the token as Authorization: Bearer <token>";

/// The Bearer secret of `req`: `Ok(None)` without an Authorization header, `Err` when it has one
/// that is no Bearer token. The scheme is matched without case (RFC 9110), and a header that is
/// not understood is refused, never read as no token: locally no token is the person at this
/// machine, so a token whose scheme was misspelt would otherwise act beyond its scopes.
fn bearer(req: &Request) -> Result<Option<String>, ()> {
    let Some(value) = req.headers().get(AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| ())?.trim();
    match value.split_once(char::is_whitespace) {
        Some((scheme, token)) if scheme.eq_ignore_ascii_case("bearer") => {
            let token = token.trim();
            (!token.is_empty())
                .then(|| Some(token.to_string()))
                .ok_or(())
        }
        _ => Err(()),
    }
}

/// Removes every `x-sculptured-*` header of `req`.
fn strip_gateway_headers(req: &mut Request) {
    let gateway: Vec<_> = (req.headers().keys())
        .filter(|k| k.as_str().starts_with(GATEWAY_HEADER_PREFIX))
        .cloned()
        .collect();
    for k in gateway {
        req.headers_mut().remove(k);
    }
}

/// What a Bearer token resolves to: `Ok(None)` without one, `Err` (the answer) when it names no
/// live token.
async fn found_caller(
    store: &Store,
    bearer: Option<String>,
) -> Result<Option<Caller>, Box<Response>> {
    let Some(b) = bearer else {
        return Ok(None);
    };
    match resolve(store, &b).await {
        Ok(Some(c)) => Ok(Some(c)),
        Ok(None) => Err(Box::new(refuse(
            StatusCode::UNAUTHORIZED,
            "unknown, revoked or expired token",
        ))),
        Err(e) => Err(Box::new(refuse(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("{e:#}"),
        ))),
    }
}

/// The largest body a request to a layer that serves others may carry: axum's own limit for the
/// API's JSON bodies, which this buffering must not raise.
const BODY_LIMIT: usize = 2 * 1024 * 1024;

/// Reads `body` whole, puts its [`BodySha256`](crate::layer::BodySha256) in `parts` for the
/// layer's `authenticate`, and hands the same bytes on; 413 past [`BODY_LIMIT`].
async fn digested(
    parts: &mut axum::http::request::Parts,
    body: axum::body::Body,
) -> Result<axum::body::Body, Box<Response>> {
    let Ok(bytes) = axum::body::to_bytes(body, BODY_LIMIT).await else {
        let too_large = StatusCode::PAYLOAD_TOO_LARGE;
        return Err(Box::new(refuse(too_large, "the request body is too large")));
    };
    let sum = ring::digest::digest(&ring::digest::SHA256, &bytes);
    let mut digest = [0u8; 32];
    digest.copy_from_slice(sum.as_ref());
    parts.extensions.insert(crate::layer::BodySha256(digest));
    Ok(axum::body::Body::from(bytes))
}

/// Drops `x-sculptured-*` headers (locally), resolves a Bearer token, then asks the layer who calls.
pub async fn authenticate(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    if !state.store.layer().policy().serves_others {
        strip_gateway_headers(&mut req);
    }
    if req.uri().path() == "/api/health" {
        return next.run(req).await;
    }
    let Ok(bearer) = bearer(&req) else {
        return refuse(StatusCode::UNAUTHORIZED, BEARER_WANTED);
    };
    let found = match found_caller(&state.store, bearer).await {
        Ok(found) => found,
        Err(answer) => return *answer,
    };
    let (mut parts, mut body) = req.into_parts();
    if state.store.layer().policy().serves_others {
        body = match digested(&mut parts, body).await {
            Ok(body) => body,
            Err(answer) => return *answer,
        };
    }
    let who = state.store.layer().authenticate(&parts, found).await;
    match who {
        Ok(c) => serve(&state, c, Request::from_parts(parts, body), next).await,
        Err(r) => refuse(r.status, &r.message),
    }
}

/// Serves `req` for `c`, with the header values it reads kept for `c`'s scope.
async fn serve(state: &AppState, c: Caller, mut req: Request, next: Next) -> Response {
    let scope = crate::layer::secret_scope(state.store.layer().as_ref(), &c);
    req.extensions_mut().insert(Arc::new(c));
    crate::secrets::scoped(scope, next.run(req)).await
}
