//! Maps a project experiment onto the engine: its world's package runs it, its scenario's preset
//! task is played with the scenario's faults as disruptions and its timed events as disruptions
//! of the parts they name, and each setup becomes a team (led, on a stepped clock). Until worlds
//! run from their files alone, this is the bridge from files to runs.

use std::collections::HashMap;

use chrono::{NaiveDateTime, NaiveTime};
use parts::proto::{ChosenDisruption, ScenarioRequest, SteppedClock, TimedDisruption};
use serde_json::{json, Value};
use shakedown_project::format::{
    parse_duration_ms, Auth, ClockSpec, ExperimentFile, Kind, ScenarioFile, SetupFile,
    Team as FileTeam, WorldFile,
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

/// The package and task deploying experiment `id` runs (so the caller can look up the task's
/// `clock_start` for [`request`]).
pub(crate) fn runtime_of(project: &Project, id: &Id) -> Result<(String, String), OpError> {
    let exp = experiment_of(project, id)?;
    runtime(
        named::<WorldFile>(project, &exp.world)?,
        named::<ScenarioFile>(project, &exp.scenario)?,
    )
}

/// The engine experiment that deploying experiment `id` starts. `clock_start` is the task's
/// (`describe`): world times in the scenario (`HH:MM` on its day, or an ISO time) are placed
/// against it.
pub(crate) fn request(
    project: &Project,
    id: &Id,
    creds: &Credentials,
    clock_start: Option<&str>,
) -> Result<NewExperimentReq, OpError> {
    let exp = experiment_of(project, id)?;
    let scenario = named::<ScenarioFile>(project, &exp.scenario)?;
    let (package, task) = runtime(named::<WorldFile>(project, &exp.world)?, scenario)?;
    let start = WorldStart::of(clock_start);
    let clock = stepped_clock(scenario, &start)?;
    let stepped = clock.is_some();
    let setups = exp
        .setups
        .iter()
        .map(|s| setup_req(project, s, creds, stepped))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(NewExperimentReq {
        name: Some(exp.label.clone()),
        task,
        scenario: scenario_req(scenario, &start, clock)?,
        runs: i32::try_from(exp.repeats.per_setup).unwrap_or(i32::MAX),
        setups,
        max_cost_usd: exp.budget.per_run_usd,
        benchmark: Some(package),
        seed: None,
        world_minutes: None,
        project_experiment: Some(id.to_string()),
    })
}

/// The id and file of the world experiment `id` runs.
pub(crate) fn world_of<'a>(
    project: &'a Project,
    id: &Id,
) -> Result<(&'a str, &'a WorldFile), OpError> {
    let exp = experiment_of(project, id)?;
    Ok((exp.world.as_str(), named::<WorldFile>(project, &exp.world)?))
}

/// The package that runs the project's world `name`.
pub(crate) fn world_package(project: &Project, name: &str) -> Result<String, OpError> {
    package_of(named::<WorldFile>(project, name)?)
}

/// The package that runs `world`: an installed benchmark or built world.
fn package_of(world: &WorldFile) -> Result<String, OpError> {
    world.package.clone().ok_or_else(|| {
        let why = "has no runtime yet: set package: to an installed benchmark or built world";
        unprocessable(format!("{} {why}", world.label))
    })
}

/// The package that runs the world and the task of it the scenario plays.
fn runtime(world: &WorldFile, scenario: &ScenarioFile) -> Result<(String, String), OpError> {
    let package = package_of(world)?;
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

/// The run's scenario: the faults as the package's disruptions, the timed events as disruptions
/// of their parts, the generators and the stepped clock. None when it has none of them.
fn scenario_req(
    scenario: &ScenarioFile,
    start: &WorldStart,
    clock: Option<SteppedClock>,
) -> Result<Option<ScenarioRequest>, OpError> {
    let req = ScenarioRequest {
        disruptions: disruptions(scenario),
        timed: timed(scenario, start)?,
        generators: scenario.generators.clone(),
        clock,
        world_data: None,
    };
    Ok((req != ScenarioRequest::default()).then_some(req))
}

/// The task's clock 0 in world local time, when it gives one (its UTC offset plays no part:
/// scenario times are world local times).
struct WorldStart(Option<NaiveDateTime>);

impl WorldStart {
    fn of(clock_start: Option<&str>) -> WorldStart {
        let start = clock_start.and_then(crate::world_clock::ClockStart::parse);
        WorldStart(start.map(|c| c.local))
    }

    /// `text` (`HH:MM` on the start's day, or `YYYY-MM-DDTHH:MM[:SS]`) as a world time and as
    /// the branch clock (ms since the start). `what` names the value in an error.
    fn place(&self, text: &str, what: &str) -> Result<(String, i64), OpError> {
        let start = self.0.ok_or_else(|| {
            unprocessable(format!(
                "{what} is a world time, and the task gives no clock_start to place it against"
            ))
        })?;
        let t = text.trim();
        let at = NaiveTime::parse_from_str(t, "%H:%M")
            .map(|time| start.date().and_time(time))
            .or_else(|_| NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S"))
            .or_else(|_| NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M"))
            .map_err(|_| {
                unprocessable(format!(
                    "{what} `{t}` is not a world time: write HH:MM or YYYY-MM-DDTHH:MM"
                ))
            })?;
        let ms = (at - start).num_milliseconds();
        Ok((at.format("%Y-%m-%dT%H:%M:%S").to_string(), ms))
    }
}

/// The scenario's stepped clock in branch-clock ms; None for a wall clock.
fn stepped_clock(
    scenario: &ScenarioFile,
    start: &WorldStart,
) -> Result<Option<SteppedClock>, OpError> {
    let Some(spec) = scenario.clock.as_ref().filter(|c| c.is_stepped()) else {
        return Ok(None);
    };
    let bad = |e: String| unprocessable(format!("{}: clock: {e}", scenario.label));
    let cost = &spec.action_cost;
    let tools = (cost.tools.iter())
        .map(|(t, d)| parse_duration_ms(d).map(|ms| (t.clone(), ms)))
        .collect::<Result<_, _>>()
        .map_err(bad)?;
    let horizon = match &spec.horizon {
        Some(h) => Some(start.place(h, "the clock's horizon")?.1),
        None => None,
    };
    let team_start = match &spec.team_start {
        Some(t) => Some(start.place(t, "the clock's team_start")?.1),
        None => None,
    };
    if let (Some(t), Some(h)) = (team_start, horizon) {
        if t >= h {
            return Err(bad("team_start must come before the horizon".into()));
        }
    }
    Ok(Some(SteppedClock {
        read_ms: parse_duration_ms(&cost.read).map_err(bad)?,
        write_ms: parse_duration_ms(&cost.write).map_err(bad)?,
        tools,
        epoch_ms: ClockSpec::epoch_ms(spec).map_err(bad)?,
        horizon_ms: horizon,
        team_start_ms: team_start.filter(|t| *t > 0),
    }))
}

/// The scenario's events that name a part and a world time, each as a disruption of that part:
/// `{id, at, known_at, at_ms, known_at_ms, do, text}` (`known_at` defaults to `at`, and `at` to
/// `known_at`).
fn timed(scenario: &ScenarioFile, start: &WorldStart) -> Result<Vec<TimedDisruption>, OpError> {
    let mut ids: Vec<String> = vec![];
    let mut out = vec![];
    for e in &scenario.events {
        let Some(part) = &e.part else { continue };
        let (Some(at), Some(known)) = (
            e.at.as_ref().or(e.known_at.as_ref()),
            e.known_at.as_ref().or(e.at.as_ref()),
        ) else {
            continue;
        };
        let (at, at_ms) = start.place(at, &format!("{}: at", e.name))?;
        let (known_at, known_ms) = start.place(known, &format!("{}: known_at", e.name))?;
        let id = unique(slug(&e.name), &ids);
        ids.push(id.clone());
        let disruption = json!({"id": id, "at": at, "known_at": known_at, "at_ms": at_ms,
            "known_at_ms": known_ms, "do": e.do_.clone().unwrap_or(Value::Null), "text": e.name});
        out.push(TimedDisruption {
            part: part.clone(),
            disruption,
        });
    }
    Ok(out)
}

/// `name` in lower case, with every run of other characters than letters and digits made `-`.
fn slug(name: &str) -> String {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    match words.is_empty() {
        true => "event".into(),
        false => words.join("-"),
    }
}

/// `id`, or `id-2`, `id-3`, … when `taken` has it.
fn unique(id: String, taken: &[String]) -> String {
    if !taken.contains(&id) {
        return id;
    }
    (2..)
        .map(|n| format!("{id}-{n}"))
        .find(|c| !taken.contains(c))
        .unwrap()
}

/// The scenario's database and system faults, as the package's disruptions.
fn disruptions(scenario: &ScenarioFile) -> Vec<ChosenDisruption> {
    scenario
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
        .collect()
}

/// `name`'s setup as an engine setup: on a stepped clock it runs as a dispatch team (its lead,
/// else its first agent, leads).
fn setup_req(
    project: &Project,
    name: &str,
    creds: &Credentials,
    stepped: bool,
) -> Result<SetupReq, OpError> {
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
        runner: match stepped {
            true => Runner::Dispatch,
            false => Runner::Chain,
        },
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
    use shakedown_project::format::AgentRuntime;
    let kind = match spec.kind {
        AgentRuntime::Builtin => AgentKind::Open,
        AgentRuntime::ClaudeCode => AgentKind::ClaudeCode,
    };
    let mut agent = AgentSpec::new(spec.id.clone(), kind);
    agent.model = Some(spec.model.clone());
    agent.prompt = spec.role.clone();
    agent.tools = spec
        .tools
        .iter()
        .filter(|t| t.as_str() != "*")
        .cloned()
        .collect();
    agent.reasoning = spec
        .reasoning
        .as_ref()
        .and_then(|r| serde_json::to_value(r).ok());
    agent.max_tokens = spec.max_tokens;
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
