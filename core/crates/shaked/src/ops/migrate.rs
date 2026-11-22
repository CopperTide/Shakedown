//! Moving earlier work into a project: installed benchmarks and built worlds become worlds that
//! run on their package, and each earlier experiment becomes a scenario, its setups and an
//! experiment file, the old record becoming that file's first deployment. Nothing is overwritten.

use std::collections::BTreeSet;

use shakedown_project::format::{
    AgentSpec as FileAgent, ExperimentFile, Kind, Repeats, ScenarioFile, SetupFile,
    Team as FileTeam, WorldFile,
};
use shakedown_project::{Id, ItemBody, Project};

use crate::model::{AgentKind, AgentSpec, Team};

/// An earlier experiment, with the team of each of its setups.
#[derive(Debug, Clone)]
pub struct OldExperiment {
    pub id: String,
    pub name: String,
    pub task: String,
    pub benchmark: String,
    pub runs: i32,
    pub teams: Vec<Team>,
}

/// What a move would do.
#[derive(Debug, Default)]
pub struct Plan {
    /// Files to write, none of which exists yet.
    pub files: Vec<(Kind, Id, ItemBody)>,
    /// Earlier experiments to link to the experiment file made from them.
    pub links: Vec<(String, Id)>,
    /// What can't be carried, in plain words.
    pub skipped: Vec<String>,
}

impl Plan {
    fn has(&self, project: &Project, kind: Kind, id: &Id) -> bool {
        project.get(kind, id).is_some() || self.files.iter().any(|(k, i, _)| *k == kind && i == id)
    }

    fn add(&mut self, project: &Project, kind: Kind, id: Id, body: ItemBody) {
        if !self.has(project, kind, &id) {
            self.files.push((kind, id, body));
        }
    }
}

/// The move of `benchmarks` (name, title) and `old` experiments into `project`.
pub fn plan(project: &Project, benchmarks: &[(String, String)], old: &[OldExperiment]) -> Plan {
    let mut p = Plan::default();
    for (name, title) in benchmarks {
        add_world(&mut p, project, name, title);
    }
    for exp in old {
        add_experiment(&mut p, project, exp);
    }
    p
}

/// `s` as an id: lowercase letters, digits and dashes, at most `max` long.
fn slug(s: &str, max: usize) -> String {
    let raw: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed: String = raw
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let cut: String = trimmed.chars().take(max).collect();
    let cut = cut.trim_end_matches('-').to_string();
    if cut.is_empty() {
        "item".into()
    } else {
        cut
    }
}

fn id_of(s: &str) -> Id {
    Id::parse(&slug(s, 64)).unwrap_or_else(|_| Id::parse("item").expect("a valid id"))
}

fn add_world(p: &mut Plan, project: &Project, name: &str, title: &str) {
    let id = id_of(name);
    if project.get(Kind::World, &id).is_some() {
        p.skipped.push(format!(
            "The world {id} already exists; it is kept as it is."
        ));
        return;
    }
    let world = WorldFile {
        format: "shakedown-world/3".into(),
        label: if title.is_empty() {
            name.into()
        } else {
            title.into()
        },
        package: Some(name.into()),
        connectors: vec![],
        flows: vec![],
        data: vec![],
        people: vec![],
        observability: None,
        views: vec![],
    };
    p.add(project, Kind::World, id, ItemBody::World(world));
}

fn add_experiment(p: &mut Plan, project: &Project, exp: &OldExperiment) {
    let setups: Vec<Id> = exp
        .teams
        .iter()
        .filter_map(|t| add_setup(p, project, t))
        .collect();
    if setups.is_empty() {
        let why = "none of its setups could be carried, so it stays under Earlier experiments.";
        p.skipped.push(format!("{}: {why}", exp.name));
        return;
    }
    let world = id_of(&exp.benchmark);
    let scenario = id_of(&format!("{}-{}", exp.benchmark, exp.task));
    p.add(
        project,
        Kind::Scenario,
        scenario.clone(),
        ItemBody::Scenario(scenario_file(exp, &world)),
    );
    let id = experiment_id(exp);
    let file = experiment_file(exp, &world, &scenario, &setups);
    p.add(
        project,
        Kind::Experiment,
        id.clone(),
        ItemBody::Experiment(file),
    );
    p.links.push((exp.id.clone(), id));
}

/// The experiment's id: its name, made unique with the start of its old id.
fn experiment_id(exp: &OldExperiment) -> Id {
    let suffix: String = exp
        .id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(6)
        .collect::<String>()
        .to_lowercase();
    id_of(&format!("{}-{suffix}", slug(&exp.name, 50)))
}

fn experiment_file(
    exp: &OldExperiment,
    world: &Id,
    scenario: &Id,
    setups: &[Id],
) -> ExperimentFile {
    let setups: BTreeSet<String> = setups.iter().map(|s| s.to_string()).collect();
    ExperimentFile {
        format: "shakedown-experiment/1".into(),
        label: exp.name.clone(),
        world: world.to_string(),
        scenario: scenario.to_string(),
        setups: setups.into_iter().collect(),
        repeats: Repeats {
            per_setup: u32::try_from(exp.runs.max(1)).unwrap_or(1),
            paired_seeds: true,
        },
        budget: Default::default(),
        parent: None,
    }
}

fn scenario_file(exp: &OldExperiment, world: &Id) -> ScenarioFile {
    ScenarioFile {
        format: "shakedown-scenario/1".into(),
        label: format!("Task {} of {}", exp.task, exp.benchmark),
        world: world.to_string(),
        task: format!("Task {} of {}", exp.task, exp.benchmark),
        preset: Some(exp.task.clone()),
        clock: None,
        changes: vec![],
        events: vec![],
        generators: vec![],
        gates: vec![],
        checks: vec![],
    }
}

/// The setup made from `team`, or None (with a note) when it can't be carried.
fn add_setup(p: &mut Plan, project: &Project, team: &Team) -> Option<Id> {
    let id = id_of(&team.id);
    match team_of(team) {
        Ok(file_team) => {
            let setup = SetupFile {
                format: "shakedown-setup/1".into(),
                label: team.name.clone(),
                team: file_team,
            };
            p.add(project, Kind::Setup, id.clone(), ItemBody::Setup(setup));
            Some(id)
        }
        Err(why) => {
            p.skipped.push(format!("{}: {why}", team.name));
            None
        }
    }
}

fn team_of(team: &Team) -> Result<FileTeam, String> {
    let Some(first) = team.agents.first().map(|a| a.kind) else {
        return Err("the team has no agents.".into());
    };
    if team.agents.iter().any(|a| a.kind != first) {
        return Err("the team mixes kinds of agents (mixed teams are described in MADL).".into());
    }
    let one = team.agents.len() == 1;
    match first {
        AgentKind::Open => Ok(FileTeam::Builtin {
            agents: team.agents.iter().map(builtin).collect(),
        }),
        AgentKind::ClaudeCode if one => Ok(FileTeam::ClaudeCode),
        AgentKind::Codex if one => Ok(FileTeam::Codex),
        AgentKind::Http if one => Ok(FileTeam::Http {
            url: team.agents[0].url.clone().unwrap_or_default(),
            auth: None,
        }),
        AgentKind::External => {
            Err("Your code setups need their command; add them as code setups.".into())
        }
        _ => Err(
            "several agents of this kind can't be one setup yet (describe the team in MADL)."
                .into(),
        ),
    }
}

fn builtin(a: &AgentSpec) -> FileAgent {
    FileAgent {
        id: a.id.clone(),
        role: a.prompt.clone(),
        model: a.model.clone().unwrap_or_else(|| "default".into()),
        key: a.provider.clone().unwrap_or_else(|| "default".into()),
        tools: if a.tools.is_empty() {
            vec!["*".into()]
        } else {
            a.tools.clone()
        },
    }
}

#[cfg(test)]
mod tests;
