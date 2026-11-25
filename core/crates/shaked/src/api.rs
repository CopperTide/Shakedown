//! The HTTP API the UI talks to. All JSON except the SSE stream.

use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Bytes,
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::{
    app::AppState,
    auth::{Caller, Who},
    gc,
    model::*,
    prompts,
    providers::{self, ProviderStatus},
    runner,
};

pub use crate::app::on_path;

mod assistant;
mod branches;
mod compare;
mod connect;
mod connections;
mod error;
pub(crate) mod experiments;
mod external;
mod extract;
mod flow;
mod me;
mod models;
mod names;
mod recording;
mod review;
pub mod routes;
mod rows;
mod runs;
mod sandbox;
mod stream;
mod suites;
mod tokens;
mod twin;
mod ui;
mod world;
mod worlds;

use error::ApiResult;
pub use error::{authorize, ApiError};
use extract::{JsonBody, OptionalJson, StartPermit};
pub use ui::with_ui;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/providers", get(list_providers))
        .route("/api/tasks", get(tasks))
        .route("/api/team/default", get(default_team))
        .route("/api/teams", get(suggested_teams))
        .route("/api/compare", get(compare::compare_branches))
        .route("/api/world", get(world::world))
        .route("/api/usage", get(world::usage))
        .route("/api/gc", post(world::gc))
        .merge(modules())
        .route("/api/{*rest}", get(api_not_found).post(api_not_found))
}

/// The routes each `api` module serves.
fn modules() -> Router<Arc<AppState>> {
    Router::new()
        .merge(runs::router())
        .merge(branches::router())
        .merge(assistant::router())
        .merge(external::router())
        .merge(connect::router())
        .merge(connections::router())
        .merge(flow::router())
        .merge(me::router())
        .merge(tokens::router())
        .merge(models::router())
        .merge(suites::router())
        .merge(experiments::router())
        .merge(review::router())
        .merge(sandbox::router())
        .merge(twin::router())
        .merge(recording::router())
        .merge(worlds::router())
        .merge(crate::ops::http::router())
}

async fn api_not_found() -> ApiError {
    ApiError::not_found("no such route")
}

// ---------------------------------------------------------------- shared helpers

/// The branch `id`; 404 when there is none.
async fn branch_or_404(state: &AppState, caller: &Caller, id: &str) -> Result<Branch, ApiError> {
    state
        .store
        .at(caller)
        .find_branch(id)
        .await?
        .ok_or_else(|| ApiError::missing("branch", id))
}

/// The query's `a` and `b` (a pair to compare); 400 without both.
fn pair(q: &HashMap<String, String>) -> Result<(String, String), ApiError> {
    match (q.get("a"), q.get("b")) {
        (Some(a), Some(b)) => Ok((a.clone(), b.clone())),
        _ => Err(ApiError::bad("a and b are required")),
    }
}

fn parse_body<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|e| ApiError::bad(format!("bad request body: {e}")))
}

/// A blank provider means the default one, as in a fork change.
fn settle_providers(team: &mut Team) {
    for a in &mut team.agents {
        a.provider = a.provider.as_deref().and_then(trimmed);
    }
}

/// The providers `caller`'s open agents run on (your keys applied), for [`team`]'s checks.
async fn providers_of(state: &AppState, caller: &Caller) -> providers::ProviderConfig {
    providers::keys::config_for(state, caller).await
}

/// Runs `f` on its own task and waits for it, so a client disconnect can't cancel it midway.
async fn detached<T: Send + 'static>(
    f: impl std::future::Future<Output = Result<T, ApiError>> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::spawn(f).await.map_err(|e| {
        let why = format!("request task failed: {e}");
        ApiError::new(axum::http::StatusCode::INTERNAL_SERVER_ERROR, why)
    })?
}

/// 507 when the Shakedown's saves already use at least the configured limit.
async fn check_disk(state: &AppState) -> Result<(), ApiError> {
    if state.saves_over_limit().await? {
        return Err(ApiError::insufficient_storage(SAVES_FULL));
    }
    Ok(())
}

/// Why a run is refused while the saves are over the limit.
pub const SAVES_FULL: &str = "Shakedown's saves use more than the limit; delete old runs";

/// The benchmark a new run, suite or experiment is made on: the one it asks for (409 when it isn't
/// installed), else the current one. Read once, so that what the request checks and what it starts
/// are on the same benchmark whatever is chosen meanwhile.
fn benchmark_asked(state: &AppState, asked: Option<&str>) -> Result<String, ApiError> {
    match asked {
        Some(name) if !state.has_benchmark(name) => Err(ApiError::conflict(format!(
            "no benchmark named {name} is installed"
        ))),
        Some(name) => Ok(name.to_string()),
        None => Ok(state.benchmark()),
    }
}

/// A request's seed: absent, or at most 2^53 - 1 (else 400).
fn checked_seed(asked: Option<u64>) -> Result<Option<u64>, ApiError> {
    match asked {
        Some(s) if s > runner::MAX_SEED => Err(ApiError::bad("seed must be at most 2^53 - 1")),
        s => Ok(s),
    }
}

/// A request's spend limit: absent, or a positive number of dollars (else 400).
fn max_cost(asked: Option<f64>) -> Result<Option<f64>, ApiError> {
    match asked {
        None => Ok(None),
        Some(v) => crate::budget::positive(v)
            .map(Some)
            .ok_or_else(|| ApiError::bad("max_cost_usd must be a positive number of dollars")),
    }
}

/// A request's world time window: absent, or from 0 to 1440 minutes (else 400).
fn world_minutes(asked: Option<f64>) -> Result<Option<f64>, ApiError> {
    match asked {
        Some(m) if !crate::window::valid_minutes(m) => Err(ApiError::bad(crate::window::INVALID)),
        m => Ok(m),
    }
}

// ---------------------------------------------------------------- simple routes

async fn health(State(state): State<Arc<AppState>>) -> ApiResult<Value> {
    let bytes = gc::usage(&state.hub, &state.benchmark()).await?;
    let benchmark = match state.bench().await {
        Ok(b) => json!({"name": b.name, "version": b.version, "tasks": b.tasks.len(),
                        "world": b.world.iter().map(|p| json!({"part": p.part, "integration": p.integration})).collect::<Vec<_>>()}),
        Err(e) => json!({"name": state.benchmark(), "error": format!("{e:#}")}),
    };
    Ok(Json(json!({
        "open_available": state.config.open_available(),
        "providers": providers::summary(&state.config.providers),
        "claude": on_path("claude"),
        "codex": on_path("codex"),
        // Packages and agents run only in sandboxes: this machine's PATH says nothing of the CLIs.
        "sandboxes": !state.store.layer().policy().runs_packages_here,
        "benchmark": benchmark,
        "disk": {"bytes": bytes, "limit_bytes": state.config.max_db_bytes},
    })))
}

/// `[{id, kind, label, available, why?, models?}]`: each provider, asking the ones that list
/// their models (each bounded by `providers::LIST_TIMEOUT`, all at once).
async fn list_providers(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
) -> Json<Vec<ProviderStatus>> {
    let config = providers_of(&state, &caller).await;
    Json(providers::statuses(&config, &state.http).await)
}

/// The benchmark's tasks: `[{id, reason, ticket, instructions, clock_start?}]`; an external design
/// gives its agents the ticket, the instructions and the world's date (`clock_start` plus the
/// branch clock) as shaked's own runners do.
async fn tasks(State(state): State<Arc<AppState>>) -> ApiResult<Value> {
    let bench = state.bench().await?;
    let task = |t: &parts::proto::TaskInfo| {
        let mut j = json!({"id": t.id, "reason": t.reason, "ticket": t.ticket,
                           "instructions": bench.instructions});
        if let Some(c) = &t.clock_start {
            j["clock_start"] = json!(c);
        }
        j
    };
    Ok(Json(bench.tasks.iter().map(task).collect()))
}

/// The team the benchmark suggests, or shaked's generic one; `kind`, `model` and `provider`
/// in the query set every agent's (Claude Code with `sonnet` when absent).
async fn default_team(
    State(state): State<Arc<AppState>>,
    Query(q): Query<prompts::Pick>,
) -> ApiResult<Team> {
    Ok(Json(prompts::default_team_with(&state.bench().await?, &q)))
}

/// Every team the benchmark suggests, the default first, each with the runner it is written for
/// (`{...team, runner}`); the query as for `default_team`.
async fn suggested_teams(
    State(state): State<Arc<AppState>>,
    Query(q): Query<prompts::Pick>,
) -> ApiResult<Vec<prompts::Suggested>> {
    Ok(Json(prompts::suggested_teams(&state.bench().await?, &q)))
}
