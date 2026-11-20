//! The HTTP API the UI talks to. All JSON except the SSE stream.

use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use parts::proto::Mode;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    app::AppState,
    fork::{self, ForkError},
    gc, metrics,
    model::*,
    prices, prompts,
    providers::{self, ProviderStatus},
    runner,
};

mod compare;
mod connect;
mod connections;
mod experiments;
mod external;
mod extract;
mod flow;
mod names;
mod recording;
mod review;
mod rows;
mod sandbox;
mod stream;
mod suites;
mod twin;
mod ui;
mod world;
mod worlds;

use extract::{JsonBody, OptionalJson, StartPermit};
pub use ui::with_ui;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/providers", get(list_providers))
        .route("/api/tasks", get(tasks))
        .route("/api/team/default", get(default_team))
        .route("/api/teams", get(suggested_teams))
        .route("/api/runs", get(list_runs).post(create_run))
        .route("/api/runs/{id}", axum::routing::delete(delete_run))
        .route("/api/runs/{id}/name", post(names::set))
        .route("/api/branches/{id}", get(get_branch))
        .route("/api/branches/{id}/stream", get(stream::stream))
        .route("/api/branches/{id}/rows", get(rows::rows))
        .route("/api/branches/{id}/fork", post(fork_branch))
        .route("/api/branches/{id}/restore-plan", get(get_restore_plan))
        .route("/api/compare", get(compare::compare_branches))
        .route("/api/world", get(world::world))
        .route("/api/usage", get(world::usage))
        .route("/api/gc", post(world::gc))
        .merge(external::router())
        .merge(connect::router())
        .merge(connections::router())
        .merge(flow::router())
        .merge(suites::router())
        .merge(experiments::router())
        .merge(review::router())
        .merge(sandbox::router())
        .merge(twin::router())
        .merge(recording::router())
        .merge(worlds::router())
        .route("/api/{*rest}", get(api_not_found).post(api_not_found))
}

// ---------------------------------------------------------------- errors

#[derive(Debug)]
pub struct ApiError(StatusCode, String);

impl ApiError {
    fn bad(msg: impl Into<String>) -> ApiError {
        ApiError(StatusCode::BAD_REQUEST, msg.into())
    }
    fn not_found(msg: impl Into<String>) -> ApiError {
        ApiError(StatusCode::NOT_FOUND, msg.into())
    }
    fn insufficient_storage(msg: impl Into<String>) -> ApiError {
        ApiError(StatusCode::INSUFFICIENT_STORAGE, msg.into())
    }
    fn conflict(msg: impl Into<String>) -> ApiError {
        ApiError(StatusCode::CONFLICT, msg.into())
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> ApiError {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
    }
}

impl From<runner::RunError> for ApiError {
    fn from(e: runner::RunError) -> ApiError {
        match e {
            runner::RunError::UnknownTask(_) => ApiError::not_found(e.to_string()),
            runner::RunError::Other(e) => e.into(),
        }
    }
}

impl From<TeamError> for ApiError {
    fn from(e: TeamError) -> ApiError {
        ApiError::bad(e.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> ApiError {
        anyhow::Error::from(e).into()
    }
}

impl From<ForkError> for ApiError {
    fn from(e: ForkError) -> ApiError {
        let status = match &e {
            ForkError::UnknownBranch(_) => StatusCode::NOT_FOUND,
            ForkError::BranchRunning
            | ForkError::Refused(_)
            | ForkError::ReplayDiverged { .. }
            | ForkError::FingerprintMismatch { .. } => StatusCode::CONFLICT,
            ForkError::UnknownSeq
            | ForkError::TeamChange(_)
            | ForkError::UnknownAgent(_)
            | ForkError::MissingHandoff(_)
            | ForkError::NoRestorePoint { .. } => StatusCode::BAD_REQUEST,
            ForkError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError(status, e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

async fn api_not_found() -> ApiError {
    ApiError::not_found("no such route")
}

/// 404 for the branch `id` that isn't there.
fn no_branch(id: &str) -> ApiError {
    ApiError::not_found(format!("there is no branch \"{id}\""))
}

/// The branch `id`; 404 when there is none.
async fn branch_or_404(state: &AppState, id: &str) -> Result<Branch, ApiError> {
    state
        .store
        .find_branch(id)
        .await?
        .ok_or_else(|| no_branch(id))
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

/// 400 when the team the fork would run (`changes` applied to `parent`) puts a changed agent in
/// open mode on a provider that can't run it.
fn check_fork_team(
    state: &AppState,
    parent: &Team,
    changes: &HashMap<String, Change>,
) -> Result<(), ApiError> {
    let team = fork::apply_changes(parent, changes);
    let changed_open = team
        .agents
        .iter()
        .filter(|a| a.kind == AgentKind::Open && changes.contains_key(&a.id));
    for a in changed_open {
        team::open_check(
            providers_of(state),
            a.provider.as_deref(),
            a.model.as_deref(),
        )?;
    }
    Ok(())
}

/// A blank provider means the default one, as in a fork change.
fn settle_providers(team: &mut Team) {
    for a in &mut team.agents {
        a.provider = a.provider.as_deref().and_then(trimmed);
    }
}

/// The providers open agents run on, for [`team`]'s checks.
fn providers_of(state: &AppState) -> &providers::ProviderConfig {
    &state.config.providers
}

/// Runs `f` on its own task and waits for it, so a client disconnect can't cancel it midway.
async fn detached<T: Send + 'static>(
    f: impl std::future::Future<Output = Result<T, ApiError>> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::spawn(f).await.map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("request task failed: {e}"),
        )
    })?
}

// ---------------------------------------------------------------- simple routes

pub fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| {
        std::env::split_paths(&p).any(|d| {
            let f = d.join(name);
            f.is_file() && {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::metadata(&f).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
                }
                #[cfg(not(unix))]
                {
                    true
                }
            }
        })
    })
}

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
        "benchmark": benchmark,
        "disk": {"bytes": bytes, "limit_bytes": state.config.max_db_bytes},
    })))
}

/// `[{id, kind, label, available, why?, models?}]`: each provider, asking the ones that list
/// their models (each bounded by `providers::LIST_TIMEOUT`, all at once).
async fn list_providers(State(state): State<Arc<AppState>>) -> Json<Vec<ProviderStatus>> {
    Json(providers::statuses(&state.config.providers, &state.http).await)
}

/// 507 when the arena's saves already use at least the configured limit.
async fn check_disk(state: &AppState) -> Result<(), ApiError> {
    if state.saves_over_limit().await? {
        return Err(ApiError::insufficient_storage(SAVES_FULL));
    }
    Ok(())
}

/// Why a run is refused while the saves are over the limit.
pub const SAVES_FULL: &str = "Shakedown's saves use more than the limit; delete old runs";

/// True when any branch of the run is running (stored as running, or live in this process).
async fn run_is_running(state: &AppState, id: &str) -> Result<bool, ApiError> {
    if state.store.run_has_running(id).await? {
        return Ok(true);
    }
    let branches = state.store.run_branches(id).await?;
    Ok(branches.iter().any(|b| state.live.contains(b)))
}

async fn delete_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    if !state.store.run_exists(&id).await? {
        return Err(ApiError::not_found(format!("there is no run \"{id}\"")));
    }
    if run_is_running(&state, &id).await? {
        return Err(ApiError::conflict("the run still has a running branch"));
    }
    detached(async move {
        let dropped = gc::delete_run(&state.store, &state.hub, &id).await?;
        Ok(Json(json!({"dropped": dropped})))
    })
    .await
}

/// The benchmark's tasks: `[{id, reason, ticket, instructions, clock_start?}]`; an external design
/// gives its agents the ticket, the instructions and the world's date (`clock_start` plus the
/// branch clock) as arenad's own runners do.
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

/// The team the benchmark suggests, or arenad's generic one; `kind`, `model` and `provider`
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

async fn list_runs(State(state): State<Arc<AppState>>) -> ApiResult<Vec<RunSummary>> {
    Ok(Json(state.store.runs().await?))
}

// ---------------------------------------------------------------- runs

#[derive(Deserialize)]
struct NewRun {
    task_id: String,
    team: Team,
    /// `chain` (default), `concurrent`, `dispatch`, or `external` (see `api::external`).
    #[serde(default)]
    runner: Runner,
    /// The disruptions to add to the task (`{disruptions: [{id, params}]}`).
    #[serde(default)]
    scenario: Option<parts::proto::ScenarioRequest>,
    /// The world's seed (at most 2^53 - 1); random when absent.
    #[serde(default)]
    seed: Option<u64>,
    /// The run's model-spend limit in USD (`crate::budget`); `ARENA_MAX_COST_USD` when absent.
    #[serde(default)]
    max_cost_usd: Option<f64>,
    /// A Code connection: arenad launches its driver, which drives the run (`runner` is then
    /// `external`; `chain`, the default, is taken as that).
    #[serde(default)]
    connection: Option<String>,
    /// The benchmark it is made on (absent: the current one); 409 when it isn't installed.
    #[serde(default)]
    benchmark: Option<String>,
    /// A world time window in minutes for a run an external system drives (`crate::window`).
    #[serde(default)]
    world_minutes: Option<f64>,
}

/// A request's world time window: absent, or from 0 to 1440 minutes (else 400).
pub(crate) fn world_minutes(asked: Option<f64>) -> Result<Option<f64>, ApiError> {
    match asked {
        Some(m) if !crate::window::valid_minutes(m) => Err(ApiError::bad(crate::window::INVALID)),
        m => Ok(m),
    }
}

/// The benchmark a new run, suite or experiment is made on: the one it asks for (409 when it isn't
/// installed), else the current one. Read once, so that what the request checks and what it starts
/// are on the same benchmark whatever is chosen meanwhile.
pub(super) fn benchmark_asked(state: &AppState, asked: Option<&str>) -> Result<String, ApiError> {
    match asked {
        Some(name) if !state.has_benchmark(name) => Err(ApiError::conflict(format!(
            "no benchmark named {name} is installed"
        ))),
        Some(name) => Ok(name.to_string()),
        None => Ok(state.benchmark()),
    }
}

/// A request's spend limit: absent, or a positive number of dollars (else 400).
pub(crate) fn max_cost(asked: Option<f64>) -> Result<Option<f64>, ApiError> {
    match asked {
        None => Ok(None),
        Some(v) => crate::budget::positive(v)
            .map(Some)
            .ok_or_else(|| ApiError::bad("max_cost_usd must be a positive number of dollars")),
    }
}

/// `POST /api/runs` with `connection` (see `connections::create_run`).
async fn create_code_run(
    state: Arc<AppState>,
    permit: StartPermit,
    headers: &axum::http::HeaderMap,
    req: NewRun,
    bench: String,
    scenario: crate::scenario::Scenario,
) -> ApiResult<Value> {
    let max_cost_usd = max_cost(req.max_cost_usd)?;
    let world_minutes = world_minutes(req.world_minutes)?;
    let (task_id, team) = (req.task_id, req.team);
    let run = connections::CodeRun {
        task_id,
        team,
        scenario,
        max_cost_usd,
        world_minutes,
        benchmark: bench,
    };
    let id = req.connection.unwrap_or_default();
    connections::create_run(state, permit, headers, (&id, req.runner), run).await
}

/// `POST /api/runs` with `runner: external` (see `external::create_run`): its spend limit and
/// world time window checked.
async fn create_external_run(
    state: Arc<AppState>,
    permit: StartPermit,
    req: NewRun,
    bench: String,
    scenario: crate::scenario::Scenario,
) -> ApiResult<Value> {
    let rules = Rules::new(Runner::External)
        .with_max_cost(max_cost(req.max_cost_usd)?)
        .with_world_minutes(world_minutes(req.world_minutes)?);
    let run = (req.task_id, req.team);
    external::create_run(state, permit, bench, run, scenario, rules).await
}

async fn create_run(
    State(state): State<Arc<AppState>>,
    permit: StartPermit,
    headers: axum::http::HeaderMap,
    JsonBody(mut req): JsonBody<NewRun>,
) -> ApiResult<Value> {
    settle_providers(&mut req.team);
    let bench = benchmark_asked(&state, req.benchmark.as_deref())?;
    let mut scenario = twin::scenario_of(&state, &bench, req.scenario.as_ref()).await?;
    if req.seed.is_some_and(|s| s > runner::MAX_SEED) {
        return Err(ApiError::bad("seed must be at most 2^53 - 1"));
    }
    scenario.seed = req.seed;
    if req.connection.is_some() {
        return create_code_run(state, permit, &headers, req, bench, scenario).await;
    }
    if req.runner == Runner::External {
        return create_external_run(state, permit, req, bench, scenario).await;
    }
    team::validate_builtin(providers_of(&state), &req.team, req.runner)?;
    let max = max_cost(req.max_cost_usd)?;
    permit.check_disk(&state).await?;
    let (run_id, branch_id) = detached(async move {
        let _permit = permit;
        let (task, team) = (&req.task_id, req.team);
        let run = runner::RunOptions::new(req.runner).scenario(&scenario);
        let run = run.benchmark(&bench);
        Ok(run.max_cost(max).start(&state, task, team).await?)
    })
    .await?;
    Ok(Json(json!({"run_id": run_id, "branch_id": branch_id})))
}

async fn get_branch(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    let branch = branch_or_404(&state, &id).await?;
    let events = state.store.events(&id).await?;
    let now = chrono::Utc::now();
    let m = metrics::metrics(&branch, &events, prices::table(), now);
    let mut v = serde_json::to_value(branch)?;
    v["budget"] = budget_of(&state, &id, &events);
    v["events"] = serde_json::to_value(events)?;
    v["metrics"] = serde_json::to_value(m)?;
    v["paused"] = json!(flow::is_paused(&state, &id));
    Ok(Json(v))
}

/// A branch's spend limit (from its `RunStart`) and, while it runs, its live estimate.
fn budget_of(state: &AppState, id: &str, events: &[crate::model::Event]) -> Value {
    let start = crate::model::RunStartPayload::of(events);
    let max = start.and_then(|s| s.max_cost_usd);
    let live = state.live.get(id).map(|c| c.budget.spent());
    json!({"max_cost_usd": max, "estimate_usd": live})
}

fn exact() -> Mode {
    Mode::Exact
}

#[derive(Deserialize)]
struct ForkReq {
    at_seq: i64,
    #[serde(default)]
    changes: HashMap<String, Change>,
    /// `"exact"` (default) or `"approximate"`.
    #[serde(default = "exact")]
    restore: Mode,
    /// Another team, for a run an external system drives (in place of `changes`).
    #[serde(default)]
    team: Option<Team>,
}

/// The branch to fork: 404 for an unknown branch and 409 for a running one; these come before
/// the 507 check.
async fn forkable(state: &AppState, id: &str) -> Result<Branch, ApiError> {
    match state.store.find_branch(id).await? {
        None => Err(no_branch(id)),
        Some(b) if b.status == BranchStatus::Running => {
            Err(ApiError::conflict("the branch is still running"))
        }
        Some(b) => Ok(b),
    }
}

/// A fork of a run a Code connection drove starts its driver again: 415/403 for a request a web
/// page could make (`connections::guard_launch`).
async fn guard_code_fork(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    id: &str,
) -> Result<(), ApiError> {
    let events = state.store.events(id).await?;
    match crate::recorded::rules(&events).connection {
        Some(_) => connections::guard_launch(state, headers),
        None => Ok(()),
    }
}

async fn fork_branch(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    permit: StartPermit,
    headers: axum::http::HeaderMap,
    JsonBody(mut req): JsonBody<ForkReq>,
) -> ApiResult<fork::ForkResponse> {
    let parent = forkable(&state, &id).await?;
    guard_code_fork(&state, &headers, &id).await?;
    fork::settle_changes(&parent.team, &mut req.changes, &state.config.providers);
    check_fork_team(&state, &parent.team, &req.changes)?;
    if let Some(team) = &mut req.team {
        settle_providers(team);
        team::validate(providers_of(&state), team)?;
    }
    permit.check_disk(&state).await?;
    let req = fork::ForkRequest {
        at_seq: req.at_seq,
        changes: req.changes,
        restore: req.restore,
        team: req.team,
    };
    let resp = detached(async move {
        let _permit = permit;
        Ok(fork::fork_with(&state, &id, req).await?)
    })
    .await?;
    Ok(Json(resp))
}

/// `restore=exact|approximate` (default exact).
fn parse_restore(q: &HashMap<String, String>) -> Result<Mode, ApiError> {
    match q.get("restore").filter(|r| !r.is_empty()) {
        None => Ok(exact()),
        Some(r) => serde_json::from_value(json!(r))
            .map_err(|_| ApiError::bad(format!("restore must be exact or approximate, not {r:?}"))),
    }
}

/// `{parts: [{part, how, estimate_seconds?}]}`: how each part of a fork at `at_seq` would be
/// restored, or the refusal (409).
async fn get_restore_plan(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Value> {
    let at_seq: i64 = q
        .get("at_seq")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ApiError::bad("at_seq must be an integer"))?;
    let mode = parse_restore(&q)?;
    let parts =
        detached(async move { Ok(fork::restore_preview(&state, &id, at_seq, mode).await?) })
            .await?;
    Ok(Json(json!({"parts": parts})))
}
