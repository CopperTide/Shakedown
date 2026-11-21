//! `/api/models/keys`: your model keys and OpenAI-compatible servers (Settings → Models). A key's
//! secret goes in and never comes back out; Test makes a one-token call with it.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{ApiError, ApiResult, JsonBody};
use crate::{
    app::AppState,
    auth::{Caller, Who},
    providers::keys,
    store::{KeyRow, NameTaken, NewKey},
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/models/keys", get(list).post(add))
        .route("/api/models/keys/{id}", axum::routing::delete(remove))
        .route("/api/models/keys/{id}/secret", post(replace))
        .route("/api/models/keys/{id}/test", post(test))
}

#[derive(Deserialize)]
struct AddKey {
    name: String,
    kind: String,
    #[serde(default)]
    secret: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    prices: Option<Value>,
}

#[derive(Deserialize)]
struct NewSecret {
    secret: String,
}

fn vault_error(e: String) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, e)
}

fn blank_to_none(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

async fn list(State(state): State<Arc<AppState>>, Who(caller): Who) -> ApiResult<Vec<KeyRow>> {
    Ok(Json(state.store.at(&caller).list_keys().await?))
}

/// `POST /api/models/keys` `{name, kind, secret, base_url?, model?, prices?}`: the key as lists
/// show it. 409 when the name is taken; 400 when the key or its server is refused.
async fn add(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    JsonBody(req): JsonBody<AddKey>,
) -> ApiResult<KeyRow> {
    let (name, base_url) = (req.name.trim(), blank_to_none(req.base_url));
    let model = blank_to_none(req.model).filter(|m| m.len() <= 100);
    let policy = state.store.layer().policy();
    let checked = keys::check_new(name, &req.kind, &req.secret, base_url.as_deref(), policy)
        .await
        .map_err(ApiError::bad)?;
    let vault = state.vault().map_err(vault_error)?;
    let k = NewKey {
        name,
        kind: checked.kind,
        secret: &req.secret,
        base_url: base_url.as_deref(),
        model: model.as_deref(),
        prices: req.prices.as_ref(),
        last4: &checked.last4,
    };
    match state.store.at(&caller).add_key(&vault, k).await {
        Ok(row) => Ok(Json(row)),
        Err(e) => match e.downcast_ref::<NameTaken>() {
            Some(taken) => Err(ApiError::conflict(taken.to_string())),
            None => Err(e.into()),
        },
    }
}

/// Key `id` of `caller`, else 404.
async fn find(state: &AppState, caller: &Caller, id: &str) -> Result<(Uuid, KeyRow), ApiError> {
    let gone = || ApiError::not_found(format!("there is no key \"{id}\""));
    let uuid: Uuid = id.parse().map_err(|_| gone())?;
    let rows = state.store.at(caller).list_keys().await?;
    let row = rows.into_iter().find(|r| r.id == uuid).ok_or_else(gone)?;
    Ok((uuid, row))
}

/// `POST /api/models/keys/{id}/secret` `{secret}`: replaces the key's secret.
async fn replace(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
    JsonBody(req): JsonBody<NewSecret>,
) -> ApiResult<Value> {
    let (uuid, row) = find(&state, &caller, &id).await?;
    let last4 = keys::check_secret(row.kind, &req.secret).map_err(ApiError::bad)?;
    let vault = state.vault().map_err(vault_error)?;
    let records = state.store.at(&caller);
    match records
        .replace_key_secret(&vault, uuid, &req.secret, &last4)
        .await?
    {
        true => Ok(Json(json!({"replaced": id, "key_last4": last4}))),
        false => Err(ApiError::not_found(format!("there is no key \"{id}\""))),
    }
}

/// `POST /api/models/keys/{id}/test`: a one-token call with the key; the key with the outcome.
async fn test(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<KeyRow> {
    let (uuid, _) = find(&state, &caller, &id).await?;
    match keys::test_key(&state, &caller, uuid)
        .await
        .map_err(vault_error)?
    {
        Some(row) => Ok(Json(row)),
        None => Err(ApiError::not_found(format!("there is no key \"{id}\""))),
    }
}

async fn remove(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    let (uuid, _) = find(&state, &caller, &id).await?;
    match state.store.at(&caller).delete_key(uuid).await? {
        true => Ok(Json(json!({"deleted": id}))),
        false => Err(ApiError::not_found(format!("there is no key \"{id}\""))),
    }
}
