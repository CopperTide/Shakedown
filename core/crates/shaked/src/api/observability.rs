//! Settings → Observability: where runs are exported (`/api/observability/targets`: Langfuse,
//! Arize Phoenix, any OTLP collector; headers go in and never come back out, Test sends one
//! span), what was exported (`/api/observability/exports`), exporting a run now
//! (`POST /api/branches/{id}/export`), and the Prometheus metrics (`GET /metrics`).

use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{ApiError, ApiResult, JsonBody, OptionalJson};
use crate::{
    app::AppState,
    auth::{Caller, Who},
    otel::{exporter, prom, send, targets},
    store::{ExportRow, NameTaken, NewTarget, TargetFlags, TargetRow},
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/observability/targets", get(list).post(add))
        .route(
            "/api/observability/targets/{id}",
            post(set_flags).delete(remove),
        )
        .route("/api/observability/targets/{id}/test", post(test))
        .route("/api/observability/exports", get(exports))
        .route("/api/branches/{id}/export", post(export))
        .route("/metrics", get(metrics))
}

/// What the exporter and a Test say of an endpoint they may not reach.
const ENDPOINT: &str = "an observability endpoint";

fn vault_error(e: String) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e)
}

async fn list(State(state): State<Arc<AppState>>, Who(caller): Who) -> ApiResult<Vec<TargetRow>> {
    Ok(Json(state.store.at(&caller).list_targets().await?))
}

/// `POST /api/observability/targets` ([`targets::Asked`]): the target as lists show it. 409 when
/// the name is taken; 400 when the preset lacks something or the endpoint may not be used here.
async fn add(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    JsonBody(asked): JsonBody<targets::Asked>,
) -> ApiResult<TargetRow> {
    let name = asked.name.trim();
    if name.is_empty() || name.len() > 60 {
        return Err(ApiError::bad("a name of 1 to 60 characters is required"));
    }
    let r = targets::resolve(&asked).map_err(ApiError::bad)?;
    let policy = state.store.layer().policy();
    crate::providers::reach::reach_as(&r.endpoint, policy, ENDPOINT)
        .await
        .map_err(ApiError::bad)?;
    let vault = state.vault().map_err(vault_error)?;
    let t = NewTarget {
        name,
        kind: r.kind,
        endpoint: &r.endpoint,
        project: r.project.as_deref(),
        headers: &r.headers,
        flags: r.flags,
    };
    match state.store.at(&caller).add_target(&vault, t).await {
        Ok(row) => Ok(Json(row)),
        Err(e) => match e.downcast_ref::<NameTaken>() {
            Some(taken) => Err(ApiError::conflict(taken.to_string())),
            None => Err(e.into()),
        },
    }
}

/// Target `id` of `caller`, else 404.
async fn find(state: &AppState, caller: &Caller, id: &str) -> Result<TargetRow, ApiError> {
    let gone = || ApiError::missing("observability target", id);
    let uuid: Uuid = id.parse().map_err(|_| gone())?;
    let rows = state.store.at(caller).list_targets().await?;
    rows.into_iter().find(|r| r.id == uuid).ok_or_else(gone)
}

#[derive(Deserialize)]
struct Flags {
    include_content: Option<bool>,
    auto_export: Option<bool>,
    export_metrics: Option<bool>,
}

/// `POST /api/observability/targets/{id}` `{include_content?, auto_export?, export_metrics?}`.
async fn set_flags(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
    JsonBody(f): JsonBody<Flags>,
) -> ApiResult<TargetRow> {
    let row = find(&state, &caller, &id).await?;
    let was = row.flags;
    let flags = TargetFlags {
        include_content: f.include_content.unwrap_or(was.include_content),
        auto_export: f.auto_export.unwrap_or(was.auto_export),
        export_metrics: f.export_metrics.unwrap_or(was.export_metrics),
    };
    state
        .store
        .at(&caller)
        .set_target_flags(row.id, flags)
        .await?;
    Ok(Json(TargetRow { flags, ..row }))
}

async fn remove(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    let row = find(&state, &caller, &id).await?;
    match state.store.at(&caller).delete_target(row.id).await? {
        true => Ok(Json(json!({"deleted": id}))),
        false => Err(ApiError::missing("observability target", &id)),
    }
}

/// `POST /api/observability/targets/{id}/test`: one span sent; the target with the outcome.
async fn test(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<TargetRow> {
    let row = find(&state, &caller, &id).await?;
    let vault = state.vault().map_err(vault_error)?;
    let records = state.store.at(&caller);
    let open = records.open_target(&vault, row.id).await?;
    let open = open.ok_or_else(|| ApiError::missing("observability target", &id))?;
    let policy = state.store.layer().policy();
    let reached = crate::providers::reach::reach_as(&open.row.endpoint, policy, ENDPOINT).await;
    let sent = match reached {
        Ok(()) => send::test(&state.http, &open).await,
        Err(why) => Err(why),
    };
    let detail = sent.as_ref().err().cloned().unwrap_or_default();
    let checked = records
        .record_target_check(row.id, sent.is_ok(), &detail)
        .await?;
    checked
        .map(Json)
        .ok_or_else(|| ApiError::missing("observability target", &id))
}

/// `GET /api/observability/exports?branch=&limit=`: the latest exports (of one branch).
async fn exports(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Vec<ExportRow>> {
    let limit = q
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(50i64)
        .clamp(1, 500);
    let branch = q.get("branch").map(String::as_str);
    Ok(Json(state.store.at(&caller).exports(branch, limit).await?))
}

#[derive(Deserialize)]
struct ExportAsk {
    #[serde(default)]
    target_id: Option<Uuid>,
}

/// `POST /api/branches/{id}/export` `{target_id?}`: sends the branch to that target (every target
/// when absent) now, or queues it when it has not ended; how it went per target.
async fn export(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
    OptionalJson(ask): OptionalJson<ExportAsk>,
) -> ApiResult<Value> {
    let target = ask.target_id;
    let done = exporter::export_now(&state, &caller, &id, target).await?;
    let results = done.ok_or_else(|| ApiError::missing("branch", &id))?;
    if results.is_empty() {
        let why = "no observability target: add one in Settings → Observability";
        return Err(ApiError::conflict(why));
    }
    Ok(Json(json!({ "branch_id": id, "exports": results })))
}

/// `GET /metrics`: the caller's runs as Prometheus text (an API token's org when hosted).
async fn metrics(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
) -> Result<Response, ApiError> {
    let snap = state.store.at(&caller).metrics_snapshot().await?;
    let text = prom::prometheus(&prom::families(&snap));
    let kind = "text/plain; version=0.0.4; charset=utf-8";
    Ok(([(header::CONTENT_TYPE, kind)], text).into_response())
}
