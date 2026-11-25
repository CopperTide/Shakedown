//! Experiments over HTTP: create one (`POST /api/experiments`), list them, read one, stop one.
//! Suites made on their own show up as one-setup experiments (`legacy`).

use crate::auth::{Caller, Who};
use crate::providers::ProviderConfig;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use parts::proto::ScenarioRequest;
use serde::Deserialize;

use super::{check_disk, checked_seed, detached, providers_of, settle_providers, JsonBody};
use super::{ApiError, ApiResult};
use crate::{
    app::AppState,
    experiment::{self, Detail, Listed, DEFAULT_RUNS, MAX_RUNS, MAX_SETUPS},
    model::{team, Runner, Team},
    store::{NewExperiment, NewSetup},
};

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/experiments", get(list).post(create))
        .route("/api/experiments/{id}", get(get_one))
        .route("/api/experiments/{id}/stop", post(stop))
}

fn default_runs() -> i32 {
    DEFAULT_RUNS
}

/// One agent setup: its name (`Setup A`, … when absent), team and runner (`chain` by default),
/// or a Code connection whose driver runs the team (its runner is then `external`).
#[derive(Debug, Deserialize)]
pub(crate) struct SetupReq {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default = "code_team")]
    pub(crate) team: Team,
    #[serde(default)]
    pub(crate) runner: Runner,
    #[serde(default)]
    pub(crate) connection: Option<String>,
}

/// The team of a Code setup that names none: its driver registers the agents.
fn code_team() -> Team {
    Team {
        id: "code".into(),
        name: "Code".into(),
        agents: vec![],
    }
}

/// A new experiment: the task, the disruptions every run adds to it, the runs per setup, and
/// the setups.
#[derive(Debug, Deserialize)]
pub(crate) struct NewExperimentReq {
    #[serde(default)]
    pub(crate) name: Option<String>,
    pub(crate) task: String,
    #[serde(default)]
    pub(crate) scenario: Option<ScenarioRequest>,
    #[serde(default = "default_runs")]
    pub(crate) runs: i32,
    pub(crate) setups: Vec<SetupReq>,
    /// Each run's model-spend limit in USD (`crate::budget`).
    #[serde(default)]
    pub(crate) max_cost_usd: Option<f64>,
    /// The benchmark it is made on (absent: the current one); 409 when it isn't installed.
    #[serde(default)]
    pub(crate) benchmark: Option<String>,
    /// The base of the runs' paired seeds (at most 2^53 - 1; drawn when absent).
    #[serde(default)]
    pub(crate) seed: Option<u64>,
    /// Each run's world time window in minutes (`crate::window`).
    #[serde(default)]
    pub(crate) world_minutes: Option<f64>,
    /// The project experiment file it is deployed from (set by the deploy operation, never by a
    /// request body).
    #[serde(skip)]
    pub(crate) project_experiment: Option<String>,
}

/// 400 unless shaked can run the setup itself: a built-in runner and a team it can run. The
/// error names the setup.
fn validate_setup(
    providers: &ProviderConfig,
    name: &str,
    setup: &mut SetupReq,
) -> Result<(), ApiError> {
    if setup.connection.is_some() {
        if !matches!(setup.runner, Runner::Chain | Runner::External) {
            return Err(ApiError::bad(format!(
                "{name}: a Code connection's driver runs the setup"
            )));
        }
        setup.runner = Runner::External;
        return team::validate_agents(providers, &setup.team.agents)
            .map_err(|e| ApiError::bad(format!("{name}: {e}")));
    }
    if setup.runner == Runner::External {
        return Err(ApiError::bad(format!(
            "{name}: an experiment's setups run on shaked's runners, or a Code connection's driver"
        )));
    }
    team::validate_builtin(providers, &setup.team, setup.runner)
        .map_err(|e| ApiError::bad(format!("{name}: {e}")))
}

/// The setups' names, after checking their number, the runs and each setup (400 otherwise).
fn validate(
    providers: &ProviderConfig,
    req: &mut NewExperimentReq,
) -> Result<Vec<String>, ApiError> {
    if !(1..=MAX_SETUPS).contains(&req.setups.len()) {
        return Err(ApiError::bad(format!(
            "an experiment has 1 to {MAX_SETUPS} setups"
        )));
    }
    if !(1..=MAX_RUNS).contains(&req.runs) {
        return Err(ApiError::bad(format!("runs must be from 1 to {MAX_RUNS}")));
    }
    let asked: Vec<Option<&str>> = req.setups.iter().map(|s| s.name.as_deref()).collect();
    let names = experiment::setup_names(&asked).map_err(ApiError::bad)?;
    for (name, setup) in names.iter().zip(&mut req.setups) {
        settle_providers(&mut setup.team);
        validate_setup(providers, name, setup)?;
    }
    Ok(names)
}

/// The Code setups' connections exist (404, naming the setup), and the request may start their
/// drivers (`guard_launch`).
async fn check_connections(
    state: &AppState,
    caller: &Caller,
    headers: &axum::http::HeaderMap,
    names: &[String],
    setups: &[SetupReq],
) -> Result<(), ApiError> {
    if setups.iter().any(|s| s.connection.is_some()) {
        super::connections::guard_launch(state, headers)?;
    }
    for (name, s) in names.iter().zip(setups) {
        if let Some(id) = &s.connection {
            super::connections::connection_or_404(state, caller, id)
                .await
                .map_err(|e| ApiError::new(e.status, format!("{name}: {}", e.message)))?;
        }
    }
    Ok(())
}

/// The setups to store, named.
fn new_setups<'a>(names: &'a [String], setups: &'a [SetupReq]) -> Vec<NewSetup<'a>> {
    let setup = |(name, s): (&'a String, &'a SetupReq)| NewSetup {
        name,
        team: &s.team,
        runner: s.runner,
        connection: s.connection.as_deref(),
    };
    names.iter().zip(setups).map(setup).collect()
}

/// `POST /api/experiments`: stores the experiment (a suite per setup) and starts running it.
/// Answers its detail.
async fn create(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    headers: axum::http::HeaderMap,
    JsonBody(req): JsonBody<NewExperimentReq>,
) -> ApiResult<Detail> {
    Ok(Json(
        create_experiment(&state, &caller, Some(&headers), req).await?,
    ))
}

/// Checks `req`, stores the experiment and starts running it: its detail. `headers` are the
/// request's, needed only to start Code connections' drivers (refused without them).
pub(crate) async fn create_experiment(
    state: &Arc<AppState>,
    caller: &Arc<Caller>,
    headers: Option<&axum::http::HeaderMap>,
    mut req: NewExperimentReq,
) -> Result<Detail, ApiError> {
    let names = validate(&providers_of(state, caller).await, &mut req)?;
    match headers {
        Some(h) => check_connections(state, caller, h, &names, &req.setups).await?,
        None if req.setups.iter().any(|s| s.connection.is_some()) => {
            return Err(ApiError::bad(
                "a Code connection's setup is started from the API",
            ));
        }
        None => {}
    }
    let (stored, suites) = store_experiment(state, caller, &names, &req).await?;
    experiment::spawn(state, caller.clone(), stored.id.clone(), suites);
    experiment::detail(state.store.at(caller), &stored)
        .await
        .map_err(Into::into)
}

/// Stores the checked experiment and its suites.
async fn store_experiment(
    state: &AppState,
    caller: &Caller,
    names: &[String],
    req: &NewExperimentReq,
) -> Result<(experiment::Experiment, Vec<crate::suite::Suite>), ApiError> {
    let max_cost_usd = super::max_cost(req.max_cost_usd)?;
    // The base seed: the one asked for, else a random one.
    let seed = checked_seed(req.seed)?.unwrap_or_else(crate::runner::new_seed);
    let world_minutes = super::world_minutes(req.world_minutes)?;
    let bench = super::benchmark_asked(state, req.benchmark.as_deref())?;
    super::suites::suite_tasks(state, &bench, vec![req.task.clone()]).await?;
    let scenario = super::twin::scenario_of(state, &bench, req.scenario.as_ref()).await?;
    for (name, setup) in names.iter().zip(&req.setups) {
        if let Some(why) = crate::scenario::refusal(setup.runner, &scenario) {
            return Err(ApiError::bad(format!("{name}: {why}")));
        }
    }
    check_disk(state).await?;
    let setups = new_setups(names, &req.setups);
    let new = NewExperiment {
        name: &experiment::name_of(req.name.as_deref(), names),
        task: &req.task,
        runs: req.runs,
        benchmark: &bench,
        scenario: &scenario,
        setups: &setups,
        max_cost_usd,
        seed,
        world_minutes,
        project_experiment: req.project_experiment.as_deref(),
    };
    Ok(state.store.at(caller).create_experiment(new).await?)
}

/// `GET /api/experiments`: every experiment and suite made on its own, newest first.
async fn list(State(state): State<Arc<AppState>>, Who(caller): Who) -> ApiResult<Vec<Listed>> {
    Ok(Json(experiment::list(state.store.at(&caller)).await?))
}

/// What an id names: an experiment, or a suite made on its own (a legacy one-setup experiment).
enum Found {
    Experiment(experiment::Experiment),
    Legacy(Box<crate::suite::Suite>),
}

async fn find(state: &AppState, caller: &Caller, id: &str) -> Result<Found, ApiError> {
    let records = state.store.at(caller);
    if let Some(e) = records.experiment(id).await? {
        return Ok(Found::Experiment(e));
    }
    match records.suite(id).await? {
        Some(s) if s.experiment_id.is_none() => Ok(Found::Legacy(Box::new(s))),
        _ => Err(ApiError::missing("experiment", id)),
    }
}

async fn detail_of(state: &AppState, caller: &Caller, found: Found) -> ApiResult<Detail> {
    let records = state.store.at(caller);
    let d = match found {
        Found::Experiment(e) => experiment::detail(records, &e).await?,
        Found::Legacy(s) => experiment::legacy_detail(records, *s).await?,
    };
    Ok(Json(d))
}

/// `GET /api/experiments/{id}`: the experiment with its setups' suite reports.
async fn get_one(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Detail> {
    let found = find(&state, &caller, &id).await?;
    detail_of(&state, &caller, found).await
}

/// `POST /api/experiments/{id}/stop`: no further run starts and the running one is stopped.
/// Answers the experiment's detail; 409 for a suite made on its own.
async fn stop(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Detail> {
    let Found::Experiment(_) = find(&state, &caller, &id).await? else {
        return Err(ApiError::conflict(
            "a suite made on its own is not an experiment shaked can stop",
        ));
    };
    detached(async move {
        experiment::stop(&state, &caller, &id).await?;
        let found = find(&state, &caller, &id).await?;
        detail_of(&state, &caller, found).await
    })
    .await
}
