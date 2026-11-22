//! The docs of some keys of a part as they were just after a step, or at the branch's end
//! (`GET /api/branches/{id}/rows`); see `crate::docs_at` for how a clock part's state at a step
//! is read.

use crate::auth::Who;
use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    Json,
};
use parts::proto::WorldPart;
use serde_json::{json, Value};

use super::{branch_or_404, ApiError, ApiResult};
use crate::{
    app::AppState,
    docs_at::{self, docs_at, NotThere, Point},
    model::*,
    saves::{part_named, part_refusal, world_of},
};

const MAX_KEYS: usize = 50;

/// A rows request: the step (or the end), at most `MAX_KEYS` keys (`<kind>/<id>`), and the part
/// to read.
struct RowsQuery {
    at: Point,
    keys: Vec<String>,
    part: Option<String>,
}

fn parse_rows_query(q: &HashMap<String, String>) -> Result<RowsQuery, ApiError> {
    let at = q
        .get("seq")
        .and_then(|s| Point::parse(s))
        .ok_or_else(|| ApiError::bad("seq must be an integer or final"))?;
    let keys: Vec<String> = q
        .get("keys")
        .map(String::as_str)
        .unwrap_or("")
        .split(',')
        .filter(|k| !k.is_empty())
        .map(|k| match k.split_once('/') {
            Some((kind, id)) if !kind.is_empty() && !id.is_empty() => Ok(k.to_string()),
            _ => Err(ApiError::bad(format!(
                "bad key {k:?}: expected <kind>/<id>"
            ))),
        })
        .collect::<Result<_, _>>()?;
    if keys.len() > MAX_KEYS {
        return Err(ApiError::bad(format!(
            "at most {MAX_KEYS} keys per request"
        )));
    }
    let part = q.get("part").filter(|p| !p.is_empty()).cloned();
    Ok(RowsQuery { at, keys, part })
}

/// The part a rows request reads: `?part=`, else the step's part, else the world's first part.
fn rows_part<'a>(
    asked: Option<&str>,
    step: Option<&Event>,
    world: &'a [WorldPart],
) -> Result<&'a WorldPart, ApiError> {
    match asked.or(step.and_then(|e| e.payload["part"].as_str())) {
        Some(n) => part_named(world, n)
            .ok_or_else(|| ApiError::bad(format!("there is no part {n} in this world"))),
        None => world
            .first()
            .ok_or_else(|| ApiError::bad("this branch's world has no parts")),
    }
}

/// A point with no state is a bad request saying why; a part's refusal of the keys (`-32602`) is
/// a bad request with the part's message; a save that is gone (`-32002`) a conflict.
fn rows_error(e: anyhow::Error) -> ApiError {
    if let Some(n) = e.downcast_ref::<NotThere>() {
        return ApiError::bad(n.to_string());
    }
    match part_refusal(&e) {
        Some((-32602, message)) => ApiError::bad(message),
        Some((-32002, _)) => ApiError::conflict("the saved state for that step is gone"),
        _ => e.into(),
    }
}

/// The docs of the request's keys in the part's state at the request's point.
async fn read_rows(
    state: &AppState,
    branch: &str,
    events: &[Event],
    q: &RowsQuery,
) -> Result<Value, ApiError> {
    let step = match q.at {
        Point::Step(seq) => Some(
            (events.iter().find(|e| e.seq == seq))
                .ok_or_else(|| rows_error(NotThere::NoStep.into()))?,
        ),
        Point::Final => None,
    };
    let world = world_of(&state.hub, events).await?;
    let part = rows_part(q.part.as_deref(), step, &world)?;
    if q.keys.is_empty() {
        docs_at::save_at(branch, events, &part.part, q.at).map_err(|n| rows_error(n.into()))?;
        return Ok(json!({"seq": seq_of(q.at), "rows": {}}));
    }
    let docs = docs_at(&state.hub, branch, events, part, q.at, &q.keys)
        .await
        .map_err(rows_error)?;
    Ok(json!({"seq": seq_of(q.at), "rows": docs}))
}

/// The point as the answer names it: the step's seq, or `final`.
fn seq_of(at: Point) -> Value {
    match at {
        Point::Step(seq) => json!(seq),
        Point::Final => json!("final"),
    }
}

pub(super) async fn rows(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Value> {
    let q = parse_rows_query(&q)?;
    branch_or_404(&state, &caller, &id).await?;
    let events = state.store.at(&caller).events(&id).await?;
    Ok(Json(read_rows(&state, &id, &events, &q).await?))
}
