//! The `/api` auth layer. Local mode: every request (past the Host guard) is the local owner's,
//! and inbound gateway headers are dropped. Hosted mode: an API token, else a
//! request the gateway signed for a user in an org; neither is 401, a bad signature too, and a
//! signed user who is not a member of the org is 403. World-changing routes are refused first.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use uuid::Uuid;

use super::{identity, signing, tokens, Scope};
use crate::app::AppState;

fn refuse(status: StatusCode, error: &str) -> Response {
    (status, Json(serde_json::json!({ "error": error }))).into_response()
}

/// Local mode: drop any `x-sculptured-*` header, then act as the local owner.
pub async fn local(mut req: Request, next: Next) -> Response {
    let gateway: Vec<_> = (req.headers().keys())
        .filter(|k| k.as_str().starts_with(signing::HEADER_PREFIX))
        .cloned()
        .collect();
    for k in gateway {
        req.headers_mut().remove(k);
    }
    req.extensions_mut().insert(Arc::new(super::local_owner()));
    next.run(req).await
}

/// The signed (user, org) of a request whose signature is good now; None otherwise.
fn signed_caller(secret: &[u8], req: &Request) -> Option<(Uuid, Uuid)> {
    let h = req.headers();
    let get = |name: &str| h.get(name).and_then(|v| v.to_str().ok());
    let (user, org) = (get(signing::USER_HEADER)?, get(signing::ORG_HEADER)?);
    let (ts, sig) = (
        get(signing::TS_HEADER)?.parse().ok()?,
        get(signing::SIG_HEADER)?,
    );
    let path = req
        .uri()
        .path_and_query()
        .map_or(req.uri().path(), |p| p.as_str());
    let now = chrono::Utc::now().timestamp();
    signing::verify_v2(secret, req.method().as_str(), path, user, org, ts, sig, now)
        .then(|| Some((user.parse().ok()?, org.parse().ok()?)))?
}

fn bearer(h: &HeaderMap) -> Option<&str> {
    let v = h.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    v.strip_prefix("Bearer ").map(str::trim)
}

/// Hosted mode's layer (see the module's notes).
pub async fn hosted(State(state): State<Arc<AppState>>, mut req: Request, next: Next) -> Response {
    if crate::mode::local_only_route(req.method().as_str(), req.uri().path()) {
        return refuse(StatusCode::FORBIDDEN, crate::mode::HOSTED_REFUSAL);
    }
    let asked = Asked {
        bearer: bearer(req.headers()).map(str::to_string),
        signed: (state.config.gateway_secret.as_ref()).and_then(|s| signed_caller(&s.0, &req)),
    };
    let scope = match caller_of(&state, asked).await {
        Ok(s) => s,
        Err((status, error)) => return refuse(status, &error),
    };
    if let Err((status, error)) = run_token_reach(&scope, req.uri().path()) {
        return refuse(status, error);
    }
    if !may_write(&scope, req.method(), req.uri().path()) {
        return refuse(StatusCode::FORBIDDEN, "your role or token may only read");
    }
    req.extensions_mut().insert(Arc::new(scope));
    next.run(req).await
}

/// A run, worker or ticket token reaches only its own branch's run-internal routes
/// (`/api/branches/<branch>/agents…` and `/finish`): another branch is not found, anything else
/// refused.
fn run_token_reach(scope: &Scope, path: &str) -> Result<(), (StatusCode, &'static str)> {
    let super::Principal::RunToken { branch, .. } = scope.principal() else {
        return Ok(());
    };
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    match parts.as_slice() {
        ["", "api", "branches", b, "agents", ..] | ["", "api", "branches", b, "finish"] => {
            match *b == branch {
                true => Ok(()),
                false => Err((StatusCode::NOT_FOUND, "no such branch")),
            }
        }
        _ => Err((
            StatusCode::FORBIDDEN,
            "a run token reaches only its own run",
        )),
    }
}

/// Whether the request may change anything: reads always; a caller's own tokens by their own
/// rules (`api::tokens`); every other write needs a role that writes (owner, admin, member) and,
/// for an API token, a scope that does (run or admin).
fn may_write(scope: &Scope, method: &axum::http::Method, path: &str) -> bool {
    use axum::http::Method;
    if [Method::GET, Method::HEAD, Method::OPTIONS].contains(method)
        || path.starts_with("/api/tokens")
    {
        return true;
    }
    if matches!(scope.principal(), super::Principal::RunToken { .. }) {
        return true; // its own branch's run-internal routes only (run_token_reach)
    }
    let write = super::Action::CreateOrgWide(super::ResourceKind::Run);
    super::can(
        scope.principal(),
        &write,
        &super::Target::Org(scope.org()),
        &Default::default(),
    )
}

/// What a request presents: a bearer token, and a good gateway signature's (user, org).
struct Asked {
    bearer: Option<String>,
    signed: Option<(Uuid, Uuid)>,
}

const SIGN_IN: &str = "sign in through the gateway or send an API token";

/// The caller's scope, or the status and error to refuse it with.
async fn caller_of(state: &AppState, asked: Asked) -> Result<Scope, (StatusCode, String)> {
    let no = |status, error: &str| Err((status, error.to_string()));
    if let Some(token) = &asked.bearer {
        let found = match token.starts_with(tokens::API_PREFIX) {
            true => tokens::resolve_api_token(&state.store, token).await,
            false => super::run_tokens::resolve_run_token(&state.store, token).await,
        };
        return match found {
            Ok(Some(p)) => Ok(Scope::of(p)),
            Ok(None) => no(
                StatusCode::UNAUTHORIZED,
                "the token is unknown, revoked or expired",
            ),
            Err(e) => no(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
        };
    }
    let Some((user, org)) = asked.signed else {
        return no(StatusCode::UNAUTHORIZED, SIGN_IN);
    };
    match identity::principal_of_user(&state.store, user, org).await {
        Ok(Some(p)) => Ok(Scope::of(p)),
        Ok(None) => no(StatusCode::FORBIDDEN, "not a member of this org"),
        Err(e) => no(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
    }
}
