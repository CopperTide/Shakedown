//! Display names are optional metadata: task ids and event records stay unchanged.
use crate::auth::Caller;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{ApiError, ApiResult, JsonBody};
use crate::app::AppState;

#[derive(Deserialize)]
pub(super) struct Name {
    name: String,
}

pub(super) async fn set(
    State(state): State<Arc<AppState>>,
    Caller(caller): Caller,
    Path(id): Path<String>,
    JsonBody(req): JsonBody<Name>,
) -> ApiResult<Value> {
    let name = req.name.trim();
    if name.is_empty() || name.len() > 300 {
        return Err(ApiError::bad("name must be 1 to 300 bytes"));
    }
    if !state.store.at(&caller).name_run(&id, name).await? {
        return Err(ApiError::not_found(format!("no run {id}")));
    }
    Ok(Json(json!({"id": id, "name": name})))
}
