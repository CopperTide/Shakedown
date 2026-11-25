//! Pausing, resuming and stopping a live branch (`POST /api/branches/{id}/pause`, `/resume`,
//! `/stop`), whatever drives its agents, and asking for a save point (`/save`). A branch a worker
//! in a sandbox runs is not live here: the control box holds it itself ([`holds`]), whether its
//! worker is connected or not, and tells the worker.

use crate::auth::{Caller, Who};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use super::{ApiError, ApiResult};
use crate::{
    app::AppState,
    exec::{BranchCtx, ToolExec},
    model::Hold,
    model::{SavePointPayload, SAVE_ASKED},
    runner,
    worker::holds,
};

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/branches/{id}/pause", post(pause))
        .route("/api/branches/{id}/resume", post(resume))
        .route("/api/branches/{id}/stop", post(stop))
        .route("/api/branches/{id}/save", post(save))
        .route("/api/branches/{id}/clock", get(clock))
}

/// The live branch `id`: 409 when it has ended (or never ran here), 404 when there is none.
async fn live(state: &AppState, caller: &Caller, id: &str) -> Result<Arc<BranchCtx>, ApiError> {
    if let Some(ctx) = state.live.get(id) {
        return Ok(ctx);
    }
    match state.store.at(caller).find_branch(id).await? {
        Some(_) => Err(ApiError::conflict("the run is not running")),
        None => Err(ApiError::missing("branch", id)),
    }
}

/// What a stop pressed by a person says.
const STOPPED_BY_HAND: &str = "stopped";

/// Puts branch `id` on `hold` when a sandbox's worker runs it (see [`holds`]): whether the hold
/// changed; None when it runs here, or no longer runs. 404 when the caller can't see it.
async fn relayed(
    state: &Arc<AppState>,
    caller: &Caller,
    id: &str,
    hold: Hold,
) -> Result<Option<bool>, ApiError> {
    if state.live.contains(id) {
        return Ok(None);
    }
    let at = state.store.at(caller);
    if at.find_branch(id).await?.is_none() {
        return Err(ApiError::missing("branch", id));
    }
    if !state.links.has(id) && !at.is_linked(id).await? {
        return Ok(None);
    }
    // The branch records for its link's caller once its worker started.
    let acting = state.links.started(id).and_then(|l| l.caller());
    let acting = acting.unwrap_or_else(|| Arc::new(caller.clone()));
    let changed = holds::put(state, &acting, id, hold, STOPPED_BY_HAND).await?;
    Ok(Some(changed))
}

/// Whether the branch is paused now, and whether the request changed that.
fn paused(ctx: &BranchCtx, changed: bool) -> ApiResult<Value> {
    Ok(Json(
        json!({"paused": ctx.flow.is_paused(), "changed": changed}),
    ))
}

/// `POST /api/branches/{id}/pause`: new tool calls and model calls wait, the world clock stops.
async fn pause(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    if let Some(changed) = relayed(&state, &caller, &id, Hold::Paused).await? {
        return Ok(Json(json!({"paused": true, "changed": changed})));
    }
    let ctx = live(&state, &caller, &id).await?;
    paused(&ctx, ctx.pause().await?)
}

/// `POST /api/branches/{id}/resume`: the calls that wait go on, the world clock runs again.
async fn resume(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    if let Some(changed) = relayed(&state, &caller, &id, Hold::Running).await? {
        return Ok(Json(json!({"paused": false, "changed": changed})));
    }
    let ctx = live(&state, &caller, &id).await?;
    let lasted = ctx.resume_lasting().await?;
    if let Some(by) = lasted {
        // Expiry counts only unpaused time: the branch's proxy tickets and run tokens move too.
        state.proxy.tickets.extend(&id, by);
        ctx.scoped(&state.store).extend_run_tokens(&id, by).await?;
    }
    paused(&ctx, lasted.is_some())
}

/// `POST /api/branches/{id}/stop`: the run ends `stopped`, unscored. shaked's own runner ends
/// the branch on its task; the stream says when it has. A queued cloud run ends at once.
async fn stop(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    if crate::hosting::stop_queued(&state, &caller, &id).await? {
        return Ok(Json(json!({"stopping": true})));
    }
    if relayed(&state, &caller, &id, Hold::STOPPED)
        .await?
        .is_some()
    {
        return Ok(Json(json!({"stopping": true})));
    }
    let ctx = live(&state, &caller, &id).await?;
    let stopped = super::detached(async move { Ok(runner::stop(&state, &ctx).await) }).await?;
    Ok(Json(json!({"stopping": stopped})))
}

/// `POST /api/branches/{id}/save`: saves the parts that save at points and changed, as a save
/// point: `{"seq", "saves"}` (`seq` null and `saves` empty when nothing was unsaved).
async fn save(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    let ctx = live(&state, &caller, &id).await?;
    if ctx.lifecycle.is_closed() {
        return Err(ApiError::conflict("the run is not running"));
    }
    let exec = ToolExec::new(state.store.clone());
    let made = exec.save_point(&ctx, SAVE_ASKED).await;
    let made = made.map_err(|e| ApiError::new(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let Some(e) = made else {
        return Ok(Json(json!({"seq": null, "saves": {}})));
    };
    let saves = e.decode::<SavePointPayload>().unwrap_or_default().saves;
    Ok(Json(json!({"seq": e.seq, "saves": saves})))
}

/// `GET /api/branches/{id}/clock`: the branch clock, its speed, whether it stands still, and the
/// world's time (`world_ms = world_epoch_ms + clock_ms`).
async fn clock(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    let ctx = live(&state, &caller, &id).await?;
    let epoch = crate::recorded::world_epoch_ms(&ctx.records().events().await?);
    let clock_ms = ctx.clock.now_ms();
    Ok(Json(json!({
        "clock_ms": clock_ms, "speed": ctx.clock.speed(), "paused": ctx.clock.is_paused(),
        "world_epoch_ms": epoch, "world_ms": epoch + clock_ms,
    })))
}

/// True when the live branch `id` is paused (a branch that isn't live never is).
pub(super) fn is_paused(state: &AppState, id: &str) -> bool {
    state.live.get(id).is_some_and(|c| c.flow.is_paused())
}
