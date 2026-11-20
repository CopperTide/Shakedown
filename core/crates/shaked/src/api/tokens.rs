//! `/api/tokens`: API tokens, made and revoked. A new token's
//! secret is in the answer once and never again.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{allowed, ApiError, ApiResult, JsonBody};
use crate::{
    app::AppState,
    auth::{Action, Target, TokenScope, Who},
    store::ApiTokenRow,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/tokens", get(list).post(create))
        .route("/api/tokens/{id}", delete(revoke))
}

#[derive(Deserialize)]
struct NewToken {
    name: String,
    scopes: Vec<String>,
    #[serde(default)]
    expires_days: Option<i64>,
}

async fn list(State(state): State<Arc<AppState>>, Who(caller): Who) -> ApiResult<Vec<ApiTokenRow>> {
    allowed(&caller, Action::ManageTokens, Target::Instance)?;
    Ok(Json(state.store.at(&caller).list_api_tokens().await?))
}

/// The scopes asked for: known, and at least one.
fn scopes_of(asked: &[String]) -> Result<Vec<TokenScope>, ApiError> {
    let scopes: Option<Vec<TokenScope>> = asked.iter().map(|s| TokenScope::parse(s)).collect();
    let scopes = scopes
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("scopes: one or more of read, run, admin"))?;
    Ok(scopes)
}

/// `POST /api/tokens` `{name, scopes, expires_days?}`: `{id, token, prefix}`, the token shown once.
async fn create(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    JsonBody(req): JsonBody<NewToken>,
) -> ApiResult<Value> {
    allowed(&caller, Action::ManageTokens, Target::Instance)?;
    let name = req.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(ApiError::bad("name must be 1 to 80 bytes"));
    }
    let scopes = scopes_of(&req.scopes)?;
    let days = req.expires_days.filter(|d| (1..=3650).contains(d));
    if req.expires_days.is_some() && days.is_none() {
        return Err(ApiError::bad("expires_days must be 1 to 3650"));
    }
    let expires = days.map(|d| chrono::Utc::now() + chrono::Duration::days(d));
    let minted = state
        .store
        .at(&caller)
        .create_api_token(name, &scopes, expires)
        .await?;
    Ok(Json(json!(minted)))
}

async fn revoke(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    allowed(&caller, Action::ManageTokens, Target::Instance)?;
    let gone = || ApiError::not_found(format!("there is no token \"{id}\""));
    let uuid: Uuid = id.parse().map_err(|_| gone())?;
    match state.store.at(&caller).revoke_api_token(uuid).await? {
        true => Ok(Json(json!({"revoked": id}))),
        false => Err(gone()),
    }
}
