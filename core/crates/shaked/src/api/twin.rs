//! The scenario builder's and the digital twin's routes: a benchmark's tasks and scenario
//! options (`GET /api/benchmark`), a scenario asked for checked and expanded, and a branch's
//! recorded views (`GET /api/branches/{id}/views`).

use crate::auth::{Caller, Who};
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use parts::proto::{ScenarioRequest, TaskInfo};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{ApiError, ApiResult};
use crate::{
    app::AppState,
    model::{Event, EventKind},
    ops::OpError,
    scenario::{self, Scenario},
};

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/benchmark", get(benchmark))
        .route("/api/branches/{id}/views", get(branch_views))
}

/// Which benchmark `GET /api/benchmark` describes: the package `name`, or the package of the
/// project world `world`; without either, the current one.
#[derive(Deserialize)]
struct Which {
    name: Option<String>,
    world: Option<String>,
}

/// A task as a list shows it: its id, reason and group (when it has one).
pub(super) fn task_summary(t: &TaskInfo) -> Value {
    let mut j = json!({"id": t.id, "reason": t.reason});
    if let Some(g) = &t.group {
        j["group"] = json!(g);
    }
    j
}

/// The package of the project world `world` (404 for no such world, or one without a package).
async fn world_package(state: &AppState, caller: &Caller, world: &str) -> Result<String, ApiError> {
    let project = state.project_for(caller).current().map_err(OpError::from)?;
    // An import made by `shaked import` while shaked runs is registered before it is read.
    for e in crate::git_import::register(&state.hub, &project.root).await {
        eprintln!("shaked: {e}");
    }
    crate::ops::world_package(&project, world).map_err(|e| ApiError::not_found(e.message))
}

/// The benchmark `which` names: 404 when it is not installed.
async fn benchmark_name(
    state: &AppState,
    caller: &Caller,
    which: Which,
) -> Result<String, ApiError> {
    let name = match (which.name, which.world) {
        (Some(name), _) => name,
        (None, Some(world)) => world_package(state, caller, &world).await?,
        (None, None) => return Ok(state.benchmark()),
    };
    if !state.has_benchmark(&name) {
        return Err(ApiError::not_found(format!(
            "no benchmark named {name} is installed"
        )));
    }
    Ok(name)
}

/// `GET /api/benchmark[?name=<package>|?world=<world id>]`: `{name, version, tasks: [{id,
/// reason, group?}], scenario_options}` (null when the benchmark offers no disruptions; `view`:
/// whether it draws the world's view).
async fn benchmark(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Query(which): Query<Which>,
) -> ApiResult<Value> {
    let name = benchmark_name(&state, &caller, which).await?;
    let bench = state.bench_named(&name).await?;
    let tasks: Vec<Value> = bench.tasks.iter().map(task_summary).collect();
    Ok(Json(json!({
        "name": bench.name,
        "version": bench.version,
        "tasks": tasks,
        "scenario_options": bench.scenario_options,
        "view": bench.view.is_some(),
    })))
}

/// The scenario `asked` expanded with the options of the benchmark `bench` (empty when none was
/// asked); 400 with why when it can't be.
pub(super) async fn scenario_of(
    state: &AppState,
    bench: &str,
    asked: Option<&ScenarioRequest>,
) -> Result<Scenario, ApiError> {
    let Some(asked) = asked else {
        return Ok(Scenario::default());
    };
    if asked.disruptions.is_empty() {
        return scenario::expand(None, asked).map_err(ApiError::bad);
    }
    let bench = state.bench_named(bench).await?;
    scenario::expand(bench.scenario_options.as_ref(), asked).map_err(ApiError::bad)
}

/// `[{seq, event_seq, view}]` of a branch's `world_view` events, in order: `seq` is the step the
/// view follows (`seq_of`), `event_seq` the view's own event.
pub fn views(events: &[Event]) -> Vec<Value> {
    let of = |e: &Event| {
        let seq = e.payload["seq_of"].as_i64().unwrap_or(e.seq);
        json!({"seq": seq, "event_seq": e.seq, "view": e.payload["view"]})
    };
    (events.iter())
        .filter(|e| e.kind == EventKind::WorldView)
        .map(of)
        .collect()
}

/// `GET /api/branches/{id}/views`: the branch's views, those it inherited before its fork point
/// included (a fork's record starts with its parent's events before that point).
async fn branch_views(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Vec<Value>> {
    super::branch_or_404(&state, &caller, &id).await?;
    let events = state.store.at(&caller).events(&id).await?;
    Ok(Json(views(&events)))
}
