//! Experiments over HTTP: create one (`POST /api/experiments`), list them, read one, stop one.
//! Suites made on their own show up as one-setup experiments (`legacy`).

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use parts::proto::ScenarioRequest;
use serde::Deserialize;

use super::{check_disk, detached, providers_of, settle_providers, JsonBody};
use super::{ApiError, ApiResult};
use crate::{
    app::AppState,
    experiment::{self, Detail, Listed, DEFAULT_RUNS, MAX_SETUPS},
    model::{team, Runner, Team},
    store::{NewExperiment, NewSetup},
    suite::MAX_REPEATS,
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
#[derive(Deserialize)]
struct SetupReq {
    #[serde(default)]
    name: Option<String>,
    #[serde(default = "code_team")]
    team: Team,
    #[serde(default)]
    runner: Runner,
    #[serde(default)]
    connection: Option<String>,
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
#[derive(Deserialize)]
struct NewExperimentReq {
    #[serde(default)]
    name: Option<String>,
    task: String,
    #[serde(default)]
    scenario: Option<ScenarioRequest>,
    #[serde(default = "default_runs")]
    runs: i32,
    setups: Vec<SetupReq>,
    /// Each run's model-spend limit in USD (`crate::budget`).
    #[serde(default)]
    max_cost_usd: Option<f64>,
}

/// 400 unless arenad can run the setup itself: a built-in runner and a team it can run. The
/// error names the setup.
fn validate_setup(state: &AppState, name: &str, setup: &mut SetupReq) -> Result<(), ApiError> {
    if setup.connection.is_some() {
        if !matches!(setup.runner, Runner::Chain | Runner::External) {
            return Err(ApiError::bad(format!(
                "{name}: a Code connection's driver runs the setup"
            )));
        }
        setup.runner = Runner::External;
        return team::validate_agents(providers_of(state), &setup.team.agents)
            .map_err(|e| ApiError::bad(format!("{name}: {e}")));
    }
    if setup.runner == Runner::External {
        return Err(ApiError::bad(format!(
            "{name}: an experiment's setups run on arenad's runners, or a Code connection's driver"
        )));
    }
    team::validate_builtin(providers_of(state), &setup.team, setup.runner)
        .map_err(|e| ApiError::bad(format!("{name}: {e}")))
}

/// The setups' names, after checking their number, the runs and each setup (400 otherwise).
fn validate(state: &AppState, req: &mut NewExperimentReq) -> Result<Vec<String>, ApiError> {
    if !(1..=MAX_SETUPS).contains(&req.setups.len()) {
        return Err(ApiError::bad(format!(
            "an experiment has 1 to {MAX_SETUPS} setups"
        )));
    }
    if !(1..=MAX_REPEATS).contains(&req.runs) {
        return Err(ApiError::bad(format!(
            "runs must be from 1 to {MAX_REPEATS}"
        )));
    }
    let asked: Vec<Option<&str>> = req.setups.iter().map(|s| s.name.as_deref()).collect();
    let names = experiment::setup_names(&asked).map_err(ApiError::bad)?;
    for (name, setup) in names.iter().zip(&mut req.setups) {
        settle_providers(&mut setup.team);
        validate_setup(state, name, setup)?;
    }
    Ok(names)
}

/// The Code setups' connections exist (404, naming the setup), and the request may start their
/// drivers (`guard_launch`).
async fn check_connections(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    names: &[String],
    setups: &[SetupReq],
) -> Result<(), ApiError> {
    if setups.iter().any(|s| s.connection.is_some()) {
        super::connections::guard_launch(state, headers)?;
    }
    for (name, s) in names.iter().zip(setups) {
        if let Some(id) = &s.connection {
            super::connections::connection_or_404(state, id)
                .await
                .map_err(|ApiError(code, msg)| ApiError(code, format!("{name}: {msg}")))?;
        }
    }
    Ok(())
}

/// `POST /api/experiments`: stores the experiment (a suite per setup) and starts running it.
/// Answers its detail.
async fn create(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    JsonBody(mut req): JsonBody<NewExperimentReq>,
) -> ApiResult<Detail> {
    let names = validate(&state, &mut req)?;
    check_connections(&state, &headers, &names, &req.setups).await?;
    let max_cost_usd = super::max_cost(req.max_cost_usd)?;
    super::suites::suite_tasks(&state, vec![req.task.clone()]).await?;
    let scenario = super::twin::scenario_of(&state, req.scenario.as_ref()).await?;
    check_disk(&state).await?;
    let setups: Vec<NewSetup> = (names.iter().zip(&req.setups))
        .map(|(name, s)| NewSetup {
            name,
            team: &s.team,
            runner: s.runner,
            connection: s.connection.as_deref(),
        })
        .collect();
    let current = state.benchmark();
    let new = NewExperiment {
        name: &experiment::name_of(req.name.as_deref(), &names),
        task: &req.task,
        runs: req.runs,
        benchmark: &current,
        scenario: &scenario,
        setups: &setups,
        max_cost_usd,
    };
    let (stored, suites) = state.store.create_experiment(new).await?;
    experiment::spawn(&state, stored.id.clone(), suites);
    Ok(Json(experiment::detail(&state.store, &stored).await?))
}

/// `GET /api/experiments`: every experiment and suite made on its own, newest first.
async fn list(State(state): State<Arc<AppState>>) -> ApiResult<Vec<Listed>> {
    Ok(Json(experiment::list(&state.store).await?))
}

/// What an id names: an experiment, or a suite made on its own (a legacy one-setup experiment).
enum Found {
    Experiment(experiment::Experiment),
    Legacy(Box<crate::suite::Suite>),
}

async fn find(state: &AppState, id: &str) -> Result<Found, ApiError> {
    if let Some(e) = state.store.experiment(id).await? {
        return Ok(Found::Experiment(e));
    }
    match state.store.suite(id).await? {
        Some(s) if s.experiment_id.is_none() => Ok(Found::Legacy(Box::new(s))),
        _ => Err(ApiError::not_found(format!(
            "there is no experiment \"{id}\""
        ))),
    }
}

async fn detail_of(state: &AppState, found: Found) -> ApiResult<Detail> {
    let d = match found {
        Found::Experiment(e) => experiment::detail(&state.store, &e).await?,
        Found::Legacy(s) => experiment::legacy_detail(&state.store, *s).await?,
    };
    Ok(Json(d))
}

/// `GET /api/experiments/{id}`: the experiment with its setups' suite reports.
async fn get_one(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> ApiResult<Detail> {
    let found = find(&state, &id).await?;
    detail_of(&state, found).await
}

/// `POST /api/experiments/{id}/stop`: no further run starts and the running one is stopped.
/// Answers the experiment's detail; 409 for a suite made on its own.
async fn stop(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> ApiResult<Detail> {
    let Found::Experiment(_) = find(&state, &id).await? else {
        return Err(ApiError::conflict(
            "a suite made on its own is not an experiment arenad can stop",
        ));
    };
    detached(async move {
        experiment::stop(&state, &id).await?;
        let found = find(&state, &id).await?;
        detail_of(&state, found).await
    })
    .await
}
