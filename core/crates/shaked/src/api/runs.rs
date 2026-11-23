//! Runs over HTTP: list them, delete one, and start one (`POST /api/runs`) on shaked's own
//! runners, with a Code connection's driver, or for an external system to drive. The request is
//! read once into a [`RunAsk`]; each way of starting checks the rest in its own order (see
//! `tests/precedence.rs`).

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    benchmark_asked, checked_seed, connections, detached, max_cost, names, providers_of,
    settle_providers, twin, world_minutes, ApiError, ApiResult, JsonBody, StartPermit,
};
use crate::{
    app::AppState,
    auth::{Action, Caller, ResourceKind, Target, Who},
    driver::{self, Launched},
    external::Started,
    gc,
    launch::{self, launch, LaunchSpec},
    layer::Costly,
    model::*,
    runner::{self, RunError},
    scenario::Scenario,
    store::Connection,
};

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/runs", get(list_runs).post(create_run))
        .route("/api/runs/{id}", delete(delete_run))
        .route("/api/runs/{id}/name", post(names::set))
}

async fn list_runs(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
) -> ApiResult<Vec<RunSummary>> {
    Ok(Json(state.store.at(&caller).runs().await?))
}

/// True when any branch of the run is running (stored as running, or live in this process).
async fn run_is_running(state: &AppState, caller: &Caller, id: &str) -> Result<bool, ApiError> {
    let records = state.store.at(caller);
    if records.run_has_running(id).await? {
        return Ok(true);
    }
    let branches = records.run_branches(id).await?;
    Ok(branches.iter().any(|b| state.live.contains(b)))
}

async fn delete_run(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    if !state.store.at(&caller).run_exists(&id).await? {
        return Err(ApiError::missing("run", &id));
    }
    if run_is_running(&state, &caller, &id).await? {
        return Err(ApiError::conflict("the run still has a running branch"));
    }
    detached(async move {
        let dropped = gc::delete_run(state.store.at(&caller), &state.hub, &id).await?;
        Ok(Json(json!({"dropped": dropped})))
    })
    .await
}

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
    /// The run's model-spend limit in USD (`crate::budget`); `SHAKEDOWN_MAX_COST_USD` when absent.
    #[serde(default)]
    max_cost_usd: Option<f64>,
    /// A Code connection: shaked launches its driver, which drives the run (`runner` is then
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

/// A run to start: its task, team (blank providers settled) and runner, the benchmark and
/// scenario it runs on (its seed set), and its limits as asked. Each starter checks the limits
/// where it always has.
pub(super) struct RunAsk {
    pub task_id: String,
    pub team: Team,
    pub runner: Runner,
    pub scenario: Scenario,
    /// The benchmark it runs on (read once when it was asked for).
    pub benchmark: String,
    pub max_cost_usd: Option<f64>,
    /// Its world time window in minutes (`crate::window`).
    pub world_minutes: Option<f64>,
}

impl RunAsk {
    /// `req` read: its benchmark (409 when not installed), scenario (400) and seed (400).
    async fn of(state: &AppState, mut req: NewRun) -> Result<RunAsk, ApiError> {
        settle_providers(&mut req.team);
        let benchmark = benchmark_asked(state, req.benchmark.as_deref())?;
        let mut scenario = twin::scenario_of(state, &benchmark, req.scenario.as_ref()).await?;
        scenario.seed = checked_seed(req.seed)?;
        Ok(RunAsk {
            task_id: req.task_id,
            team: req.team,
            runner: req.runner,
            scenario,
            benchmark,
            max_cost_usd: req.max_cost_usd,
            world_minutes: req.world_minutes,
        })
    }

    /// 400 for a bad spend limit, then for a bad world time window.
    fn check_limits(&self) -> Result<(), ApiError> {
        max_cost(self.max_cost_usd)?;
        world_minutes(self.world_minutes)?;
        Ok(())
    }

    /// Starts it with `rules` (and the connection whose `driver` drives it), for `caller`.
    async fn launch(
        self,
        state: &Arc<AppState>,
        caller: Arc<Caller>,
        rules: Rules,
        driver: Option<&Connection>,
    ) -> Result<launch::Launched, RunError> {
        let spec = LaunchSpec {
            scope: caller,
            task_id: &self.task_id,
            team: self.team,
            scenario: &self.scenario,
            rules,
            driver,
            benchmark: Some(&self.benchmark),
        };
        launch(state, spec).await
    }
}

/// `v` with the layer's admission `warning`, when there is one.
fn with_warning(mut v: Value, warning: Option<String>) -> Value {
    if let Some(w) = warning {
        v["warning"] = json!(w);
    }
    v
}

/// `POST /api/runs`: the layer admits and meters the run where every run starts
/// ([`launch`](crate::launch::launch)); its warning, if any, is in the answer. A run the layer
/// refuses is refused first, before its world is looked up, so the answer names the quota.
async fn create_run(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    permit: StartPermit,
    headers: HeaderMap,
    JsonBody(req): JsonBody<NewRun>,
) -> ApiResult<Value> {
    crate::layer::admit(&state.store, &caller, Costly::StartRun).await?;
    start_run(state, caller, permit, headers, req).await
}

/// The connection a run names must be one the caller may use; then the run is read, and started
/// by its connection's driver, for an external system, or on shaked's own runner.
async fn start_run(
    state: Arc<AppState>,
    caller: Arc<Caller>,
    permit: StartPermit,
    headers: HeaderMap,
    mut req: NewRun,
) -> ApiResult<Value> {
    let connection = req.connection.take();
    if let Some(id) = connection.as_deref() {
        let kind = ResourceKind::Connection;
        let named = Target::Resource { kind, id };
        super::authorize(&state, &caller, Action::UseInRun, named).await?;
    }
    let ask = RunAsk::of(&state, req).await?;
    match connection {
        Some(id) => code_run(state, caller, permit, &headers, &id, ask).await,
        None if ask.runner == Runner::External => external(state, caller, permit, ask).await,
        None => builtin(state, caller, permit, ask).await,
    }
}

/// A run on shaked's own runner: 400 for a team it can't run, then for a bad spend limit; 507
/// while the saves are over the limit. Its world time window is not read.
async fn builtin(
    state: Arc<AppState>,
    caller: Arc<Caller>,
    permit: StartPermit,
    ask: RunAsk,
) -> ApiResult<Value> {
    team::validate_builtin(&providers_of(&state, &caller).await, &ask.team, ask.runner)?;
    let max = max_cost(ask.max_cost_usd)?;
    permit.check_disk(&state).await?;
    let v = detached(async move {
        let _permit = permit;
        let run = runner::RunOptions::new(ask.runner)
            .scenario(&ask.scenario)
            .scope(&caller)
            .benchmark(&ask.benchmark)
            .max_cost(max);
        let l = run.launch(&state, &ask.task_id, ask.team).await?;
        let v = json!({"run_id": l.run_id, "branch_id": l.branch_id});
        Ok(with_warning(v, l.warning))
    })
    .await?;
    Ok(Json(v))
}

/// A run whose agents an external system drives: its world started, its clock ticking, the
/// team's agents (there may be none yet) registered. Answers `{run_id, branch_id, agents}` with
/// each agent's endpoints. The limits are checked after the team and the disk, the order
/// external runs have always been refused in.
async fn external(
    state: Arc<AppState>,
    caller: Arc<Caller>,
    permit: StartPermit,
    ask: RunAsk,
) -> ApiResult<Value> {
    team::validate_agents(&providers_of(&state, &caller).await, &ask.team.agents)?;
    permit.check_disk(&state).await?;
    ask.check_limits()?;
    let rules = Rules::new(Runner::External)
        .with_max_cost(ask.max_cost_usd)
        .with_world_minutes(ask.world_minutes);
    detached(async move {
        let _permit = permit;
        let l = ask.launch(&state, caller, rules, None).await?;
        let mut started = Started::of(l.run_id, l.branch_id, l.agents);
        started.run_token = l.run_token;
        let v = serde_json::to_value(started)?;
        Ok(Json(with_warning(v, l.warning)))
    })
    .await
}

/// `POST /api/runs` with `connection`: 400 for a bad limit, then 403/415 for a request a web page
/// could make, 400 for a runner other than `external` (or `chain`, the default), 404 for an
/// unknown connection.
async fn code_run(
    state: Arc<AppState>,
    caller: Arc<Caller>,
    permit: StartPermit,
    headers: &HeaderMap,
    id: &str,
    ask: RunAsk,
) -> ApiResult<Value> {
    ask.check_limits()?;
    connections::guard_launch(&state, headers)?;
    if !matches!(ask.runner, Runner::Chain | Runner::External) {
        return Err(ApiError::bad(
            "a run with a Code connection is driven by its driver",
        ));
    }
    let conn = connections::connection_or_404(&state, &caller, id).await?;
    Ok(Json(start_code(state, caller, permit, conn, ask).await?.0))
}

/// A run a connection's driver drives, just started: the run's ids, its agents' endpoints (none
/// yet), and whether the driver started (`launched`) or not (`failed`, the run ended saying why).
pub(super) async fn start_code(
    state: Arc<AppState>,
    caller: Arc<Caller>,
    permit: StartPermit,
    conn: Connection,
    ask: RunAsk,
) -> Result<(Value, Option<Launched>), ApiError> {
    team::validate_agents(&providers_of(&state, &caller).await, &ask.team.agents)?;
    let ids = ask.team.agents.iter().map(|a| a.id.as_str());
    driver::check_token_envs(ids).map_err(ApiError::bad)?;
    permit.check_disk(&state).await?;
    detached(async move {
        let _permit = permit;
        let rules = driver::rules_for(&conn.id, ask.max_cost_usd, ask.world_minutes);
        let l = ask.launch(&state, caller, rules, Some(&conn)).await?;
        let (started, launched) = (Started::of(l.run_id, l.branch_id, l.agents), l.driver);
        let mut v = with_warning(serde_json::to_value(&started)?, l.warning);
        v["connection"] = json!(conn.id);
        v["driver"] = json!(match launched {
            Some(_) => "launched",
            None => "failed",
        });
        Ok((v, launched))
    })
    .await
}
