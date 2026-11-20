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
    let (mut parts, body) = req.into_parts();
    let who = state.store.layer().authenticate(&parts, found).await;
    match who {
        Ok(c) => {
            parts.extensions.insert(Arc::new(c));
            next.run(Request::from_parts(parts, body)).await
        }
        Err(r) => refuse(r.status, &r.message),
    }
}
