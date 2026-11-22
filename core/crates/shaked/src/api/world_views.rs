//! A branch's world views: the view files of the project world it ran
//! (`GET /api/branches/{id}/world-views`) and one screen of one evaluated at a point
//! (`GET /api/branches/{id}/world-views/{view}?screen=&at=live|final|<seq>`). The branch's world
//! is the project world whose `package` is the run's benchmark.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use shakedown_project::{
    format::{Kind, Screen, ViewFile},
    world_views, Id, ItemBody, WorldView,
};

use super::{branch_or_404, ApiError, ApiResult};
use crate::{
    app::AppState,
    auth::{Caller, Who},
    docs_at::NotThere,
    model::Event,
    recorded::run_info,
    views::{self, At, Ctx, NotLive},
};

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/branches/{id}/world-views", get(list))
        .route("/api/branches/{id}/world-views/{view}", get(evaluate))
}

/// The project world `events`' run played (its `package` is the run's benchmark) and its views;
/// None without a project or such a world.
fn world_of_run(
    state: &AppState,
    caller: &Caller,
    events: &[Event],
) -> Option<(Id, Vec<WorldView>)> {
    let project = state.project_for(caller).current().ok()?;
    let package = run_info(events).benchmark;
    let world = project
        .list(Kind::World)
        .into_iter()
        .find_map(|item| match &item.body {
            ItemBody::World(w) if w.package.as_deref() == Some(package.as_str()) => {
                Some((item.id.clone(), w.clone()))
            }
            _ => None,
        })?;
    let views = world_views(&project.root, &world.0, &world.1);
    Some((world.0, views))
}

async fn branch_events(
    state: &AppState,
    caller: &Caller,
    id: &str,
) -> Result<Vec<Event>, ApiError> {
    branch_or_404(state, caller, id).await?;
    Ok(state.store.at(caller).events(id).await?)
}

fn view_json(id: &str, view: &ViewFile) -> Value {
    let mut v = serde_json::to_value(view).unwrap_or_else(|_| json!({}));
    v["id"] = json!(id);
    v
}

/// `{world, views: [{id, label, part, styles, panels, screens}], problems: [{file, message}]}`:
/// the views that parse, and why the others don't.
async fn list(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    let events = branch_events(&state, &caller, &id).await?;
    let Some((world, found)) = world_of_run(&state, &caller, &events) else {
        return Ok(Json(json!({"world": null, "views": [], "problems": []})));
    };
    let (mut views, mut problems) = (vec![], vec![]);
    for v in &found {
        match &v.view {
            Ok(view) => views.push(view_json(&v.id, view)),
            Err(why) => problems.push(json!({"file": v.file, "message": why})),
        }
    }
    Ok(Json(
        json!({"world": world.to_string(), "views": views, "problems": problems}),
    ))
}

/// A point that has no state, or a live view of a branch that isn't running, is said as it is.
fn read_error(e: anyhow::Error) -> ApiError {
    if let Some(n) = e.downcast_ref::<NotThere>() {
        return ApiError::bad(n.to_string());
    }
    if e.downcast_ref::<NotLive>().is_some() {
        return ApiError::conflict(NotLive.to_string());
    }
    e.into()
}

/// The view `view_id` of the branch's world.
fn view_named<'a>(found: &'a [WorldView], view_id: &str) -> Result<&'a ViewFile, ApiError> {
    (found.iter().find(|v| v.id == view_id))
        .and_then(|v| v.view.as_ref().ok())
        .ok_or_else(|| ApiError::not_found(format!("this branch's world has no view {view_id}")))
}

/// The screen `?screen=` names, else the first.
fn screen_named<'a>(
    view: &'a ViewFile,
    q: &HashMap<String, String>,
) -> Result<&'a Screen, ApiError> {
    let screen = match q.get("screen") {
        Some(s) => view.screens.iter().find(|x| x.id == *s),
        None => view.screens.first(),
    };
    screen.ok_or_else(|| ApiError::not_found("the view has no such screen"))
}

/// `?at=`, else live while the branch runs, else its end.
fn at_asked(q: &HashMap<String, String>, live: bool, events: &[Event]) -> Result<At, ApiError> {
    match q.get("at") {
        Some(a) => {
            At::parse(a).ok_or_else(|| ApiError::bad("at must be live, final or a step's seq"))
        }
        None => Ok(At::default_for(live, events)),
    }
}

/// The parts the panels of `screen` read.
fn screen_parts(view: &ViewFile, screen: &Screen) -> BTreeSet<String> {
    let names = screen.areas.iter().flat_map(|r| r.split_whitespace());
    let panels = names.filter_map(|n| view.panels.get(n));
    panels.flat_map(|p| views::parts_of(view, p)).collect()
}

/// `{view, screen, at: {point, seq, clock_ms, world_ms, speed, paused}, panels: {id: evaluated}}`.
async fn evaluate(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path((id, view_id)): Path<(String, String)>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Value> {
    let events = branch_events(&state, &caller, &id).await?;
    let found = world_of_run(&state, &caller, &events).map_or_else(Vec::new, |(_, v)| v);
    let view = view_named(&found, &view_id)?;
    let screen = screen_named(view, &q)?;
    let at = at_asked(&q, state.live.get(&id).is_some(), &events)?;
    let parts = screen_parts(view, screen);
    let read = (views::read(&state, &id, &events, &parts, at).await).map_err(read_error)?;
    let ctx = Ctx {
        view,
        docs: &read.docs,
        world_ms: read.moment.world_ms,
        events: &read.events,
    };
    let panels = views::screen(&ctx, screen);
    Ok(Json(json!({"view": view_id, "screen": screen.id,
                   "at": read.moment.to_json(), "panels": panels})))
}
