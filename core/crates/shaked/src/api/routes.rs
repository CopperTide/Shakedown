//! The route table: every `/api` route the engine serves, the action it asks for and what on. The
//! authorize middleware reads it for each request, so a route is asked about before its handler
//! runs; `tests/routes.rs` checks the table against the sources.

use std::sync::Arc;

use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};

use super::ApiError;
use crate::{
    app::AppState,
    auth::{Action, Caller, ResourceKind, Target},
};

/// What a route's action is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum On {
    Instance,
    /// Listing or creating records of a kind.
    Kind(ResourceKind),
    /// The record its `{id}` (or `{name}`) names.
    Resource(ResourceKind),
    /// The branch its `{id}` names.
    Branch,
}

/// One route of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteCase {
    pub method: &'static str,
    pub path: &'static str,
    pub action: Action,
    pub on: On,
    /// Answered without asking the layer (health, `/api/me`, the 404 of unknown routes).
    pub public: bool,
}

const fn r(method: &'static str, path: &'static str, action: Action, on: On) -> RouteCase {
    RouteCase {
        method,
        path,
        action,
        on,
        public: false,
    }
}

const fn public(method: &'static str, path: &'static str) -> RouteCase {
    RouteCase {
        method,
        path,
        action: Action::Read,
        on: On::Instance,
        public: true,
    }
}

use Action::*;
use On::{Branch, Instance};
use ResourceKind as K;

const fn kind(k: ResourceKind) -> On {
    On::Kind(k)
}

const fn one(k: ResourceKind) -> On {
    On::Resource(k)
}

pub const ROUTES: &[RouteCase] = &[
    public("GET", "/api/health"),
    public("GET", "/api/me"),
    public("GET", "/api/{*rest}"),
    public("POST", "/api/{*rest}"),
    r("GET", "/api/providers", Read, Instance),
    r("GET", "/api/tasks", Read, Instance),
    r("GET", "/api/team/default", Read, Instance),
    r("GET", "/api/teams", Read, Instance),
    r("GET", "/api/benchmark", Read, Instance),
    r("GET", "/api/world", Read, Instance),
    r("GET", "/api/usage", Read, Instance),
    r("POST", "/api/gc", InstanceAdmin, Instance),
    r("GET", "/api/review/settings", Read, Instance),
    r("POST", "/api/agents/check", Start, kind(K::Run)),
    r("GET", "/api/runs", Read, kind(K::Run)),
    r("POST", "/api/runs", Start, kind(K::Run)),
    r("DELETE", "/api/runs/{id}", Delete, one(K::Run)),
    r("POST", "/api/runs/{id}/name", Rename, one(K::Run)),
    r("GET", "/api/compare", Read, kind(K::Run)),
    r("GET", "/api/branches/{id}", Read, Branch),
    r("GET", "/api/branches/{id}/stream", Read, Branch),
    r("GET", "/api/branches/{id}/rows", Read, Branch),
    r("GET", "/api/branches/{id}/restore-plan", Read, Branch),
    r("GET", "/api/branches/{id}/clock", Read, Branch),
    r("GET", "/api/branches/{id}/recording", Read, Branch),
    r("GET", "/api/branches/{id}/recording.mp4", Read, Branch),
    r("GET", "/api/branches/{id}/views", Read, Branch),
    r("GET", "/api/branches/{id}/review", Read, Branch),
    r("POST", "/api/branches/{id}/review", Record, Branch),
    r("POST", "/api/branches/{id}/fork", Fork, Branch),
    r("POST", "/api/branches/{id}/pause", Stop, Branch),
    r("POST", "/api/branches/{id}/resume", Stop, Branch),
    r("POST", "/api/branches/{id}/stop", Stop, Branch),
    r("POST", "/api/branches/{id}/save", Record, Branch),
    r("POST", "/api/branches/{id}/agents", RegisterAgent, Branch),
    r(
        "POST",
        "/api/branches/{id}/agents/{agent}/end",
        Report,
        Branch,
    ),
    r(
        "POST",
        "/api/branches/{id}/agents/{agent}/usage",
        Report,
        Branch,
    ),
    r(
        "POST",
        "/api/branches/{id}/agents/{agent}/reasoning",
        Report,
        Branch,
    ),
    r("POST", "/api/branches/{id}/finish", Finish, Branch),
    r("POST", "/api/branches/{id}/sandbox-events", Report, Branch),
    r("GET", "/api/compare/review", Read, kind(K::Review)),
    r("POST", "/api/compare/review", Create, kind(K::Review)),
    r("GET", "/api/suites/compare/review", Read, kind(K::Review)),
    r(
        "POST",
        "/api/suites/compare/review",
        Create,
        kind(K::Review),
    ),
    r("GET", "/api/connections", Read, kind(K::Connection)),
    r("POST", "/api/connections", Create, kind(K::Connection)),
    r(
        "DELETE",
        "/api/connections/{id}",
        Delete,
        one(K::Connection),
    ),
    r(
        "POST",
        "/api/connections/{id}/test",
        UseInRun,
        one(K::Connection),
    ),
    r("GET", "/api/experiments", Read, kind(K::Experiment)),
    r("POST", "/api/experiments", Create, kind(K::Experiment)),
    r("GET", "/api/experiments/{id}", Read, one(K::Experiment)),
    r(
        "POST",
        "/api/experiments/{id}/stop",
        Stop,
        one(K::Experiment),
    ),
    r("GET", "/api/suites", Read, kind(K::Suite)),
    r("POST", "/api/suites", Create, kind(K::Suite)),
    r("GET", "/api/suites/compare", Read, kind(K::Suite)),
    r("GET", "/api/suites/{id}", Read, one(K::Suite)),
    r("POST", "/api/suites/{id}/claim", Start, one(K::Suite)),
    r("GET", "/api/tokens", ManageTokens, Instance),
    r("POST", "/api/tokens", ManageTokens, Instance),
    r("DELETE", "/api/tokens/{id}", ManageTokens, Instance),
    r("GET", "/api/templates", Read, Instance),
    r("GET", "/api/templates/{id}", Read, Instance),
    r("POST", "/api/templates/{id}/preview", Read, Instance),
    r("GET", "/api/worlds", Read, kind(K::World)),
    r("POST", "/api/worlds", Create, kind(K::World)),
    r("GET", "/api/worlds/{name}", Read, one(K::World)),
    r("POST", "/api/worlds/{name}/use", ChangeSetup, one(K::World)),
    r(
        "POST",
        "/api/worlds/{name}/prepare",
        ChangeSetup,
        one(K::World),
    ),
];

/// The row of `method` on the route pattern `path`.
pub fn case(method: &str, path: &str) -> Option<&'static RouteCase> {
    ROUTES.iter().find(|r| r.method == method && r.path == path)
}

/// The segment of `uri` at the position of the first `{…}` segment of `pattern`.
fn param<'a>(pattern: &str, uri: &'a str) -> &'a str {
    let at = pattern.split('/').position(|s| s.starts_with('{'));
    at.and_then(|i| uri.split('/').nth(i)).unwrap_or_default()
}

/// The routes that change this machine's worlds or saves: refused where packages run elsewhere.
fn changes_this_machine(c: &RouteCase) -> bool {
    matches!(
        (c.method, c.path),
        (
            "POST",
            "/api/worlds" | "/api/gc" | "/api/worlds/{name}/use" | "/api/worlds/{name}/prepare"
        )
    )
}

/// Asks about the matched route before its handler runs: the engine's checks, then the layer's.
pub async fn authorize_route(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let pattern = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string());
    let Some(c) = pattern.and_then(|p| case(req.method().as_str(), &p)) else {
        return ApiError::not_found("no such route".to_string()).into_response();
    };
    if c.public {
        return next.run(req).await;
    }
    let Some(caller) = req.extensions().get::<Arc<Caller>>().cloned() else {
        return crate::auth::extract::no_caller().into_response();
    };
    if changes_this_machine(c) && !state.store.layer().policy().runs_packages_here {
        let why = "this shaked runs packages only in sandboxes";
        return ApiError(axum::http::StatusCode::FORBIDDEN, why.into()).into_response();
    }
    let id = param(c.path, req.uri().path()).to_string();
    match ask(&state, &caller, c, &id).await {
        Ok(()) => next.run(req).await,
        Err(e) => e.into_response(),
    }
}

/// The target of `c` for `id`, and the answer to it.
async fn ask(state: &AppState, caller: &Caller, c: &RouteCase, id: &str) -> Result<(), ApiError> {
    let run = match c.on {
        On::Branch => (state.store.at(caller).find_branch(id).await?)
            .map(|b| b.run_id)
            .unwrap_or_default(),
        _ => String::new(),
    };
    let target = match c.on {
        On::Instance => Target::Instance,
        On::Kind(k) => Target::Kind(k),
        On::Resource(k) => Target::Resource { kind: k, id },
        On::Branch => Target::Branch {
            run: &run,
            branch: id,
        },
    };
    super::authorize(state, caller, c.action, target).await
}
