//! Serves every registry operation at its HTTP route. The input is the JSON body, with the query
//! parameters and the path's `{id}` added; the route table gets one row per operation so each
//! request is authorized before its handler runs.

use std::sync::{Arc, OnceLock};

use axum::{
    body::Bytes,
    extract::{Query, RawPathParams, State},
    http::Method,
    routing::{on, MethodFilter},
    Json, Router,
};
use serde_json::{Map, Value};

use super::{all, Access, OpCtx, OpError, Operation};
use crate::{
    api::{
        routes::{On, RouteCase},
        ApiError,
    },
    app::AppState,
    auth::{Action, ResourceKind, Target, Who},
};

/// A router with every operation at its method and path.
pub fn router() -> Router<Arc<AppState>> {
    all().iter().fold(Router::new(), |router, op| {
        let filter =
            MethodFilter::try_from(Method::from_bytes(op.method.as_bytes()).unwrap_or(Method::GET))
                .unwrap_or(MethodFilter::GET);
        router.route(
            op.path,
            on(filter, move |state, who, path, query, body| {
                serve(op, state, who, path, query, body)
            }),
        )
    })
}

async fn serve(
    op: &'static Operation,
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    path: RawPathParams,
    Query(query): Query<Map<String, Value>>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let mut input = parse_body(&body)?;
    for (key, value) in query {
        input.entry(key).or_insert(value);
    }
    for (key, value) in &path {
        input.insert(key.to_string(), Value::String(value.to_string()));
    }
    Ok(Json(
        (op.run)(&OpCtx::as_caller(&state, caller), op, Value::Object(input)).await?,
    ))
}

fn parse_body(body: &[u8]) -> Result<Map<String, Value>, OpError> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Map::new());
    }
    match serde_json::from_slice(body) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(OpError::bad_request("the body must be a JSON object")),
        Err(e) => Err(OpError::bad_request(e.to_string())),
    }
}

/// The route-table rows of every operation.
pub fn route_rows() -> &'static [RouteCase] {
    static ROWS: OnceLock<Vec<RouteCase>> = OnceLock::new();
    ROWS.get_or_init(|| all().iter().map(row).collect())
}

fn row(op: &Operation) -> RouteCase {
    let on = match (op.target, op.path.contains('{')) {
        (None, _) => On::Instance,
        (Some(_), false) => On::Kind(ResourceKind::ProjectFile),
        (Some(_), true) => On::Resource(ResourceKind::ProjectFile),
    };
    RouteCase {
        method: op.method,
        path: op.path,
        action: action_of(op.access),
        on,
        public: false,
    }
}

/// The action an access class asks for.
pub(super) fn action_of(access: Access) -> Action {
    match access {
        Access::Read => Action::Read,
        Access::Edit => Action::Create,
        Access::Spend => Action::Start,
        Access::Delete => Action::Delete,
    }
}

/// What an operation acts on, for item `id` (empty for none).
pub(super) fn target_of<'a>(op: &Operation, id: &'a str) -> Target<'a> {
    match (op.target, op.path.contains('{')) {
        (None, _) => Target::Instance,
        (Some(_), false) => Target::Kind(ResourceKind::ProjectFile),
        (Some(_), true) => Target::Resource {
            kind: ResourceKind::ProjectFile,
            id,
        },
    }
}
