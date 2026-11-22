//! Maps a project experiment onto the engine: its world's package runs it, its scenario's preset
//! task is played with the scenario's faults as disruptions, and each setup becomes a team. Until
//! worlds run from their files alone, this is the bridge from files to runs.

use std::collections::HashMap;

use parts::proto::{ChosenDisruption, ScenarioRequest};
use shakedown_project::format::{
    Auth, ExperimentFile, Kind, ScenarioFile, SetupFile, Team as FileTeam, WorldFile,
};
use shakedown_project::{kind_name, Id, KindFile, Project, ProjectError};

use super::{unprocessable, OpError};
use crate::api::experiments::{NewExperimentReq, SetupReq};
use crate::model::{AgentKind, AgentSpec, AuthRef, Runner, Team};
use crate::providers::KEY_PREFIX;

/// What a deploy refers to by name, and may use: for each stored key a built-in agent names, and
/// each credential an endpoint's `auth` names, `Ok` or why it may not be used. No secret is held.
#[derive(Default)]
pub(crate) struct Credentials {
    providers: HashMap<String, Result<(), String>>,
    auth: HashMap<AuthRef, Result<(), String>>,
}

impl Credentials {
    pub(crate) fn provider(&mut self, key: &str, verdict: Result<(), String>) {
        self.providers.insert(key.to_string(), verdict);
    }

    pub(crate) fn credential(&mut self, auth: AuthRef, verdict: Result<(), String>) {
        self.auth.insert(auth, verdict);
    }
}

/// What the setups of an experiment name: stored keys (as providers) and credentials.
#[derive(Default)]
pub(crate) struct Needs {
    pub providers: Vec<String>,
    pub auth: Vec<AuthRef>,
}

/// The keys and credentials the experiment's setups name, so only those are looked up.
pub(crate) fn needs(project: &Project, id: &Id) -> Result<Needs, OpError> {
    let mut out = Needs::default();
    let exp = experiment_of(project, id)?;
    for name in &exp.setups {
        match &named::<SetupFile>(project, name)?.team {
            FileTeam::Builtin { agents } => {
                let keys = agents
                    .iter()
                    .map(|a| a.key.trim())
                    .filter(|k| !k.is_empty());
                out.providers.extend(keys.map(str::to_string));
            }
            FileTeam::Http {
                auth: Some(auth), ..
            } => out.auth.extend(auth_ref(name, auth)?),
            _ => {}
        }
    }
    Ok(out)
}

/// The engine experiment that deploying experiment `id` starts.
pub(crate) fn request(
    project: &Project,
    id: &Id,
    creds: &Credentials,
) -> Result<NewExperimentReq, OpError> {
    let exp = experiment_of(project, id)?;
    let scenario = named::<ScenarioFile>(project, &exp.scenario)?;
    let (package, task) = runtime(named::<WorldFile>(project, &exp.world)?, scenario)?;
    let setups = exp
        .setups
        .iter()
        .map(|s| setup_req(project, s, creds))
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

/// Experiment `id` (404 if there is none), refused when it or anything it names has problems.
fn experiment_of<'a>(project: &'a Project, id: &Id) -> Result<&'a ExperimentFile, OpError> {
    let exp = project
        .body::<ExperimentFile>(id)
        .ok_or_else(|| ProjectError::not_found(Kind::Experiment, id))?;
    no_problems(project, id, exp)?;
    Ok(exp)
}

/// The item of format `B` that an experiment or setup names; naming none is 422.
fn named<'a, B: KindFile>(project: &'a Project, name: &str) -> Result<&'a B, OpError> {
    let missing = || unprocessable(format!("no {} named {name}", kind_name(B::KIND)));
    let id = Id::parse(name).map_err(|_| missing())?;
    project.body::<B>(&id).ok_or_else(missing)
}

/// Refuses an experiment whose own file, world, scenario or setups have problems.
fn no_problems(project: &Project, id: &Id, exp: &ExperimentFile) -> Result<(), OpError> {
    let mut refs = vec![
        (Kind::Experiment, id.to_string()),
        (Kind::World, exp.world.clone()),
        (Kind::Scenario, exp.scenario.clone()),
    ];
    refs.extend(exp.setups.iter().map(|s| (Kind::Setup, s.clone())));
    for (kind, name) in refs {
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

fn setup_req(project: &Project, name: &str, creds: &Credentials) -> Result<SetupReq, OpError> {
    let setup = named::<SetupFile>(project, name)?;
    let agents = agents_of(name, &setup.team, creds)?;
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

fn agents_of(name: &str, team: &FileTeam, creds: &Credentials) -> Result<Vec<AgentSpec>, OpError> {
    Ok(match team {
        FileTeam::Builtin { agents } => agents
            .iter()
            .map(|a| builtin_agent(name, a, creds))
            .collect::<Result<_, _>>()?,
        FileTeam::ClaudeCode => vec![AgentSpec::new("claude-code", AgentKind::ClaudeCode)],
        FileTeam::Codex => vec![AgentSpec::new("codex", AgentKind::Codex)],
        FileTeam::Http { url, auth } => {
            let mut agent = AgentSpec::new("endpoint", AgentKind::Http);
            agent.url = Some(url.clone());
            if let Some(auth) = auth {
                agent.auth = endpoint_auth(name, auth, creds)?;
            }
            vec![agent]
        }
        FileTeam::Code { .. } | FileTeam::Madl { .. } => {
            return Err(unprocessable(format!(
                "{name}: this kind of setup can't be deployed from a file yet"
            )));
        }
    })
}

/// The reference an endpoint's `auth` makes: `None` when it names nothing; naming both a key and
/// a variable is refused.
fn auth_ref(setup: &str, auth: &Auth) -> Result<Option<AuthRef>, OpError> {
    match (&auth.key, &auth.env) {
        (Some(_), Some(_)) => Err(OpError::bad_request(format!(
            "{setup}: auth names both a key and an environment variable: name one"
        ))),
        (Some(key), None) => Ok(Some(AuthRef::Key(key.clone()))),
        (None, Some(var)) => Ok(Some(AuthRef::Env(var.clone()))),
        (None, None) => Ok(None),
    }
}

/// The credential reference of an endpoint, checked: it must be one a deploy found usable.
fn endpoint_auth(
    setup: &str,
    auth: &Auth,
    creds: &Credentials,
) -> Result<Option<AuthRef>, OpError> {
    let Some(found) = auth_ref(setup, auth)? else {
        return Ok(None);
    };
    match creds.auth.get(&found) {
        Some(Ok(())) => Ok(Some(found)),
        Some(Err(why)) => Err(OpError::bad_request(format!("{setup}: auth: {why}"))),
        None => Err(OpError::bad_request(format!(
            "{setup}: auth names nothing usable"
        ))),
    }
}

/// A built-in agent. Its `key` makes `key:<name>` its provider, so it runs on that key's
/// provider and model; a key that is not stored, or may not serve you, refuses the deploy.
fn builtin_agent(
    setup: &str,
    spec: &shakedown_project::format::AgentSpec,
    creds: &Credentials,
) -> Result<AgentSpec, OpError> {
    let mut agent = AgentSpec::new(spec.id.clone(), AgentKind::Open);
    agent.model = Some(spec.model.clone());
    agent.prompt = spec.role.clone();
    agent.tools = spec
        .tools
        .iter()
        .filter(|t| t.as_str() != "*")
        .cloned()
        .collect();
    let key = spec.key.trim();
    if !key.is_empty() {
        match creds.providers.get(key) {
            Some(Ok(())) => agent.provider = Some(format!("{KEY_PREFIX}{key}")),
            Some(Err(why)) => return Err(key_refused(setup, &spec.id, key, why)),
            None => return Err(key_refused(setup, &spec.id, key, "is not stored")),
        }
    }
    Ok(agent)
}

fn key_refused(setup: &str, agent: &str, key: &str, why: &str) -> OpError {
    OpError::bad_request(format!(
        "{setup}: agent {agent} names the key {key:?}, which {why}"
    ))
}

#[cfg(test)]
mod tests;
