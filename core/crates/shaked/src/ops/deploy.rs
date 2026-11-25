//! Maps a project experiment onto the engine: its world's package runs it, its scenario's preset
//! task is played with the scenario's faults as disruptions and its timed events as disruptions
//! of the parts they name, and each setup becomes a team (led, on a stepped clock). Until worlds
//! run from their files alone, this is the bridge from files to runs.

use axum::http::StatusCode;
use chrono::{NaiveDateTime, NaiveTime};
use parts::proto::{ChosenDisruption, ScenarioRequest, SteppedClock, TimedDisruption};
use serde_json::{json, Value};
use shakedown_project::format::{
    parse_duration_ms, ClockSpec, ExperimentFile, Kind, ScenarioFile, SetupFile, Team as FileTeam,
    WorldFile,
};
use shakedown_project::{Id, ItemBody, Project};

use super::OpError;
use crate::api::experiments::{NewExperimentReq, SetupReq};
use crate::model::{AgentKind, AgentSpec, Runner, Team};

/// The package and task deploying experiment `id` runs (so the caller can look up the task's
/// `clock_start` for [`request`]).
pub(crate) fn runtime_of(project: &Project, id: &Id) -> Result<(String, String), OpError> {
    let exp = experiment_of(project, id)?;
    runtime(
        world_of(project, &exp.world)?,
        scenario_of(project, &exp.scenario)?,
    )
}

/// The engine experiment that deploying experiment `id` starts. `clock_start` is the task's
/// (`describe`): world times in the scenario (`HH:MM` on its day, or an ISO time) are placed
/// against it.
pub(crate) fn request(
    project: &Project,
    id: &Id,
    clock_start: Option<&str>,
) -> Result<NewExperimentReq, OpError> {
    let exp = experiment_of(project, id)?;
    no_problems(project, id, exp)?;
    let scenario = scenario_of(project, &exp.scenario)?;
    let (package, task) = runtime(world_of(project, &exp.world)?, scenario)?;
    let start = WorldStart::of(clock_start);
    let clock = stepped_clock(scenario, &start)?;
    let setups = exp
        .setups
        .iter()
        .map(|s| setup_req(project, s, clock.is_some()))
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
fn setup_req(project: &Project, name: &str, stepped: bool) -> Result<SetupReq, OpError> {
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
        runner: match stepped {
            true => Runner::Dispatch,
            false => Runner::Chain,
        },
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
    agent
}

#[cfg(test)]
mod tests;
