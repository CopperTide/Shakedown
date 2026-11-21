//! Maps a project experiment onto the engine: its world's package runs it, its scenario's preset
//! task is played with the scenario's faults as disruptions, and each setup becomes a team. Until
//! worlds run from their files alone, this is the bridge from files to runs.

use axum::http::StatusCode;
use parts::proto::{ChosenDisruption, ScenarioRequest};
use shakedown_project::format::{
    ExperimentFile, Kind, ScenarioFile, SetupFile, Team as FileTeam, WorldFile,
};
use shakedown_project::{Id, ItemBody, Project};

use super::OpError;
use crate::api::experiments::{NewExperimentReq, SetupReq};
use crate::model::{AgentKind, AgentSpec, Runner, Team};

/// The engine experiment that deploying experiment `id` starts.
pub(crate) fn request(project: &Project, id: &Id) -> Result<NewExperimentReq, OpError> {
    let exp = experiment_of(project, id)?;
    no_problems(project, id, exp)?;
    let scenario = scenario_of(project, &exp.scenario)?;
    let (package, task) = runtime(world_of(project, &exp.world)?, scenario)?;
    let setups = exp
        .setups
        .iter()
        .map(|s| setup_req(project, s))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(NewExperimentReq {
        name: Some(exp.label.clone()),
        task,
        scenario: disruptions(scenario),
        runs: i32::try_from(exp.repeats.per_setup).unwrap_or(i32::MAX),
        setups,
        max_cost_usd: exp.budget.per_run_usd,
        benchmark: Some(package),
        seed: None,
        world_minutes: None,
        project_experiment: Some(id.to_string()),
    })
}

/// The package that runs the world and the task of it the scenario plays.
fn runtime(world: &WorldFile, scenario: &ScenarioFile) -> Result<(String, String), OpError> {
    let package = world.package.clone().ok_or_else(|| {
        let why = "has no runtime yet: set package: to an installed benchmark or built world";
        unprocessable(format!("{} {why}", world.label))
    })?;
    let task = scenario.preset.clone().ok_or_else(|| {
        unprocessable(format!(
            "{} names no task of {package} to play: set preset:",
            scenario.label
        ))
    })?;
    Ok((package, task))
}

fn unprocessable(message: String) -> OpError {
    OpError::new(StatusCode::UNPROCESSABLE_ENTITY, message)
}

fn body<'a>(project: &'a Project, kind: Kind, name: &str) -> Result<&'a ItemBody, OpError> {
    let missing = || {
        unprocessable(format!(
            "no {} named {name}",
            shakedown_project::kind_name(kind)
        ))
    };
    let id = Id::parse(name).map_err(|_| missing())?;
    project
        .get(kind, &id)
        .map(|item| &item.body)
        .ok_or_else(missing)
}

fn experiment_of<'a>(project: &'a Project, id: &Id) -> Result<&'a ExperimentFile, OpError> {
    let not_found = || OpError::new(StatusCode::NOT_FOUND, format!("no experiment named {id}"));
    match project.get(Kind::Experiment, id).map(|i| &i.body) {
        Some(ItemBody::Experiment(e)) => Ok(e),
        _ => Err(not_found()),
    }
}

fn world_of<'a>(project: &'a Project, name: &str) -> Result<&'a WorldFile, OpError> {
    match body(project, Kind::World, name)? {
        ItemBody::World(w) => Ok(w),
        _ => Err(unprocessable(format!("{name} is not a world"))),
    }
}

fn scenario_of<'a>(project: &'a Project, name: &str) -> Result<&'a ScenarioFile, OpError> {
    match body(project, Kind::Scenario, name)? {
        ItemBody::Scenario(s) => Ok(s),
        _ => Err(unprocessable(format!("{name} is not a scenario"))),
    }
}

/// Refuses an experiment whose own file, world, scenario or setups have problems.
fn no_problems(project: &Project, id: &Id, exp: &ExperimentFile) -> Result<(), OpError> {
    let mut named = vec![
        (Kind::Experiment, id.to_string()),
        (Kind::World, exp.world.clone()),
        (Kind::Scenario, exp.scenario.clone()),
    ];
    named.extend(exp.setups.iter().map(|s| (Kind::Setup, s.clone())));
    for (kind, name) in named {
        let Ok(item) = Id::parse(&name) else { continue };
        if let Some(p) = project.problems_of(kind, &item).first() {
            return Err(unprocessable(format!(
                "fix {name} before deploying: {}",
                p.message
            )));
        }
    }
    Ok(())
}

/// The scenario's database and system faults, as the package's disruptions.
fn disruptions(scenario: &ScenarioFile) -> Option<ScenarioRequest> {
    let chosen: Vec<ChosenDisruption> = scenario
        .events
        .iter()
        .filter_map(|e| {
            let id = e.fault.clone()?;
            let params = e
                .params
                .as_ref()
                .and_then(|p| p.as_object().cloned())
                .unwrap_or_default();
            Some(ChosenDisruption { id, params })
        })
        .collect();
    (!chosen.is_empty()).then_some(ScenarioRequest {
        disruptions: chosen,
    })
}

fn setup_req(project: &Project, name: &str) -> Result<SetupReq, OpError> {
    let setup: &SetupFile = match body(project, Kind::Setup, name)? {
        ItemBody::Setup(s) => s,
        _ => return Err(unprocessable(format!("{name} is not an agent setup"))),
    };
    let agents = agents_of(name, &setup.team)?;
    let team = Team {
        id: name.to_string(),
        name: setup.label.clone(),
        agents,
    };
    Ok(SetupReq {
        name: Some(setup.label.clone()),
        team,
        runner: Runner::Chain,
        connection: None,
    })
}

fn agents_of(name: &str, team: &FileTeam) -> Result<Vec<AgentSpec>, OpError> {
    Ok(match team {
        FileTeam::Builtin { agents } => agents.iter().map(builtin_agent).collect(),
        FileTeam::ClaudeCode => vec![AgentSpec::new("claude-code", AgentKind::ClaudeCode)],
        FileTeam::Codex => vec![AgentSpec::new("codex", AgentKind::Codex)],
        FileTeam::Http { url, .. } => {
            let mut agent = AgentSpec::new("endpoint", AgentKind::Http);
            agent.url = Some(url.clone());
            vec![agent]
        }
        FileTeam::Code { .. } | FileTeam::Madl { .. } => {
            return Err(unprocessable(format!(
                "{name}: this kind of setup can't be deployed from a file yet"
            )));
        }
    })
}

fn builtin_agent(spec: &shakedown_project::format::AgentSpec) -> AgentSpec {
    let mut agent = AgentSpec::new(spec.id.clone(), AgentKind::Open);
    agent.model = Some(spec.model.clone());
    agent.prompt = spec.role.clone();
    agent.tools = spec
        .tools
        .iter()
        .filter(|t| t.as_str() != "*")
        .cloned()
        .collect();
    agent
}

#[cfg(test)]
mod tests;
