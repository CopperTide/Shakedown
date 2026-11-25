//! A branch over HTTP: read it (with its budget, events and metrics), fork it, and see how a fork
//! at a step would restore its world.

use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    branch_or_404, connections, detached, flow, providers_of, rows, settle_providers, stream,
    ApiError, ApiResult, JsonBody, StartPermit,
};
use crate::{
    app::AppState,
    auth::{Caller, Who},
    fork,
    layer::{Costly, KeySource, Meter, Usage},
    metrics,
    model::*,
    prices, providers,
};

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/branches/{id}", get(get_branch))
        .route("/api/branches/{id}/events/{seq}", get(get_event))
        .route("/api/branches/{id}/stream", get(stream::stream))
        .route("/api/branches/{id}/rows", get(rows::rows))
        .route("/api/branches/{id}/fork", post(fork_branch))
        .route("/api/branches/{id}/restore-plan", get(get_restore_plan))
}

/// `GET /api/branches/{id}`: the branch, its events, budget and metrics. `?lean=1` reads each
/// model call without its request and response bodies (`Scoped::events_lean`), for pages that
/// show the world rather than the calls: most of a long run's record, left out.
async fn get_branch(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Value> {
    let branch = branch_or_404(&state, &caller, &id).await?;
    let records = state.store.at(&caller);
    let lean = q.get("lean").is_some_and(|v| v == "1" || v == "true");
    let events = match lean {
        true => records.events_lean(&id).await?,
        false => records.events(&id).await?,
    };
    let now = chrono::Utc::now();
    let m = metrics::metrics(&branch, &events, prices::table(), now);
    let mut v = serde_json::to_value(branch)?;
    v["budget"] = budget_of(&state, &id, &events);
    v["events"] = serde_json::to_value(events)?;
    v["metrics"] = serde_json::to_value(m)?;
    v["paused"] = json!(flow::is_paused(&state, &id));
    let queued = state.hosting.get().and_then(|h| h.queue_position(&id));
    v["queue_position"] = json!(queued);
    Ok(Json(v))
}

/// Load the full body of one selected step without downloading the entire run.
async fn get_event(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path((id, seq)): Path<(String, i64)>,
) -> ApiResult<Event> {
    branch_or_404(&state, &caller, &id).await?;
    let event = state
        .store
        .at(&caller)
        .event(&id, seq)
        .await?
        .ok_or_else(|| ApiError::not_found("no such step"))?;
    Ok(Json(event))
}

/// A branch's spend limit (from its `RunStart`) and, while it runs, its live estimate.
fn budget_of(state: &AppState, id: &str, events: &[Event]) -> Value {
    let start = RunStartPayload::of(events);
    let max = start.and_then(|s| s.max_cost_usd);
    let live = state.live.get(id).map(|c| c.budget.spent());
    json!({"max_cost_usd": max, "estimate_usd": live})
}

fn exact() -> RestoreMode {
    RestoreMode::Exact
}

#[derive(Deserialize)]
struct ForkReq {
    at_seq: i64,
    #[serde(default)]
    changes: HashMap<String, Change>,
    /// `"exact"` (default), `"approximate"`, or `"none"` (evaluation only: the world as the
    /// parent ended).
    #[serde(default = "exact")]
    restore: RestoreMode,
    /// Another team, for a run an external system drives (in place of `changes`).
    #[serde(default)]
    team: Option<Team>,
    /// Disruptions to add or remove from the fork point on.
    #[serde(default)]
    disruptions: fork::DisruptionChange,
}

/// The branch to fork: 404 for an unknown branch and 409 for a running one; these come before
/// the 507 check.
async fn forkable(state: &AppState, caller: &Caller, id: &str) -> Result<Branch, ApiError> {
    match state.store.at(caller).find_branch(id).await? {
        None => Err(ApiError::missing("branch", id)),
        Some(b) if b.status.goes_on() => Err(ApiError::conflict("the branch is still running")),
        Some(b) => Ok(b),
    }
}

/// A fork of a run a Code connection drove starts its driver again: 415/403 for a request a web
/// page could make (`connections::guard_launch`).
async fn guard_code_fork(
    state: &AppState,
    caller: &Caller,
    headers: &HeaderMap,
    id: &str,
) -> Result<(), ApiError> {
    let events = state.store.at(caller).events(id).await?;
    match crate::recorded::rules(&events).connection {
        Some(_) => connections::guard_launch(state, headers),
        None => Ok(()),
    }
}

/// 400 when the team the fork would run (`changes` applied to `parent`) puts a changed agent in
/// open mode on a provider that can't run it.
fn check_fork_team(
    providers: &providers::ProviderConfig,
    parent: &Team,
    changes: &HashMap<String, Change>,
) -> Result<(), ApiError> {
    let team = fork::apply_changes(parent, changes);
    let changed_open = team
        .agents
        .iter()
        .filter(|a| a.kind == AgentKind::Open && changes.contains_key(&a.id));
    for a in changed_open {
        team::open_check(providers, a.provider.as_deref(), a.model.as_deref())?;
    }
    Ok(())
}

/// `POST /api/branches/{id}/fork`: admitted and metered (one run) by the layer.
async fn fork_branch(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
    permit: StartPermit,
    headers: HeaderMap,
    JsonBody(req): JsonBody<ForkReq>,
) -> ApiResult<fork::ForkResponse> {
    crate::layer::admit(&state.store, &caller, Costly::Fork { branch: &id }).await?;
    let (st, who) = (state.clone(), caller.clone());
    let Json(resp) = fork_admitted(st, who, id, permit, headers, req).await?;
    let usage = Usage {
        meter: Meter::Runs,
        quantity: 1,
        key: format!("fork:{}", resp.branch_id),
        run: None,
        branch: Some(&resp.branch_id),
        paid_by: KeySource::Environment,
    };
    crate::layer::meter(&state.store, &caller, usage).await;
    Ok(Json(resp))
}

async fn fork_admitted(
    state: Arc<AppState>,
    caller: Arc<Caller>,
    id: String,
    permit: StartPermit,
    headers: HeaderMap,
    mut req: ForkReq,
) -> ApiResult<fork::ForkResponse> {
    let parent = forkable(&state, &caller, &id).await?;
    guard_code_fork(&state, &caller, &headers, &id).await?;
    fork::settle_changes(&parent.team, &mut req.changes, &state.config.providers);
    let providers = providers_of(&state, &caller).await;
    check_fork_team(&providers, &parent.team, &req.changes)?;
    if let Some(team) = &mut req.team {
        settle_providers(team);
        team::validate(&providers, team)?;
    }
    permit.check_disk(&state).await?;
    let req = fork::ForkRequest {
        at_seq: req.at_seq,
        changes: req.changes,
        restore: req.restore,
        team: req.team,
        disruptions: req.disruptions,
    };
    let resp = detached(async move {
        let _permit = permit;
        Ok(fork::fork_with(&state, &caller, &id, req).await?)
    })
    .await?;
    Ok(Json(resp))
}

/// `restore=exact|approximate|none` (default exact).
fn parse_restore(q: &HashMap<String, String>) -> Result<RestoreMode, ApiError> {
    match q.get("restore").filter(|r| !r.is_empty()) {
        None => Ok(exact()),
        Some(r) => serde_json::from_value(json!(r)).map_err(|_| {
            ApiError::bad(format!(
                "restore must be exact, approximate or none, not {r:?}"
            ))
        }),
    }
}

/// `{parts: [{part, how, estimate_seconds?}]}`: how each part of a fork at `at_seq` would be
/// restored, or the refusal (409).
async fn get_restore_plan(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Value> {
    let at_seq: i64 = q
        .get("at_seq")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ApiError::bad("at_seq must be an integer"))?;
    let mode = parse_restore(&q)?;
    let parts =
        detached(
            async move { Ok(fork::restore_preview(&state, &caller, &id, at_seq, mode).await?) },
        )
        .await?;
    Ok(Json(json!({"parts": parts})))
}
