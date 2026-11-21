//! Planning the move of earlier work into a project: what becomes which file, what is linked and
//! what can't be carried.

use std::path::Path;

use shakedown_project::format::{Kind, Team as FileTeam};
use shakedown_project::{Id, ItemBody, Project};

use super::{plan, OldExperiment};
use crate::model::{AgentKind, AgentSpec, Team};

fn empty_project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::init(dir.path(), "P", false).unwrap();
    (dir, project)
}

fn open_agent(id: &str) -> AgentSpec {
    let mut a = AgentSpec::new(id, AgentKind::Open);
    a.model = Some("m".into());
    a.prompt = format!("You are {id}.");
    a
}

fn team(id: &str, agents: Vec<AgentSpec>) -> Team {
    Team {
        id: id.into(),
        name: format!("Team {id}"),
        agents,
    }
}

fn old(id: &str, name: &str, teams: Vec<Team>) -> OldExperiment {
    OldExperiment {
        id: id.into(),
        name: name.into(),
        task: "t1".into(),
        benchmark: "toybench".into(),
        runs: 3,
        teams,
    }
}

fn ids(p: &super::Plan, kind: Kind) -> Vec<String> {
    p.files
        .iter()
        .filter(|(k, _, _)| *k == kind)
        .map(|(_, id, _)| id.to_string())
        .collect()
}

const BENCH: &[(&str, &str)] = &[("toybench", "Toy bench")];

fn benches() -> Vec<(String, String)> {
    BENCH
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn benchmarks_become_worlds() {
    let (_d, p) = empty_project();
    let plan = plan(&p, &benches(), &[]);
    let (_, _, body) = &plan.files[0];
    match body {
        ItemBody::World(w) => assert_eq!(
            (w.label.as_str(), w.package.as_deref()),
            ("Toy bench", Some("toybench"))
        ),
        other => panic!("{other:?}"),
    }
}

/// The first built-in agent of the plan's setup: its model and key.
fn first_agent(plan: &super::Plan) -> (String, String) {
    let setup = plan
        .files
        .iter()
        .find(|(k, _, _)| *k == Kind::Setup)
        .expect("a setup");
    match &setup.2 {
        ItemBody::Setup(s) => match &s.team {
            FileTeam::Builtin { agents } => (agents[0].model.clone(), agents[0].key.clone()),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn experiment_becomes_files_and_link() {
    let (_d, p) = empty_project();
    let solo = team("solo", vec![open_agent("planner")]);
    let plan = plan(
        &p,
        &benches(),
        &[old("abcdef123", "Solo vs team", vec![solo])],
    );
    assert_eq!(ids(&plan, Kind::Scenario), ["toybench-t1"]);
    assert_eq!(ids(&plan, Kind::Setup), ["solo"]);
    assert_eq!(ids(&plan, Kind::Experiment), ["solo-vs-team-abcdef"]);
    let expected = Id::parse("solo-vs-team-abcdef").unwrap();
    assert_eq!(plan.links, vec![("abcdef123".to_string(), expected)]);
    assert_eq!(first_agent(&plan), ("m".to_string(), "default".to_string()));
}

#[test]
fn code_setup_skipped() {
    let (_d, p) = empty_project();
    let code = team("crew", vec![AgentSpec::new("driver", AgentKind::External)]);
    let plan = plan(&p, &benches(), &[old("abcdef123", "Crew", vec![code])]);
    assert!(ids(&plan, Kind::Setup).is_empty());
    assert!(
        plan.skipped
            .iter()
            .any(|s| s.contains("Your code setups need their command")),
        "{:?}",
        plan.skipped
    );
    assert!(
        ids(&plan, Kind::Experiment).is_empty(),
        "an experiment with no setup left is skipped too"
    );
}

#[test]
fn mixed_team_skipped() {
    let (_d, p) = empty_project();
    let mixed = team(
        "mixed",
        vec![open_agent("a"), AgentSpec::new("cc", AgentKind::ClaudeCode)],
    );
    let plan = plan(&p, &benches(), &[old("abcdef123", "Mixed", vec![mixed])]);
    assert!(ids(&plan, Kind::Setup).is_empty());
    assert!(
        plan.skipped.iter().any(|s| s.contains("mixed")),
        "{:?}",
        plan.skipped
    );
}

#[test]
fn same_names_get_distinct_ids() {
    let (_d, p) = empty_project();
    let t = || vec![team("solo", vec![open_agent("planner")])];
    let plan = plan(
        &p,
        &benches(),
        &[old("aaaaaa1", "Same", t()), old("bbbbbb2", "Same", t())],
    );
    assert_eq!(ids(&plan, Kind::Experiment), ["same-aaaaaa", "same-bbbbbb"]);
    assert_eq!(
        ids(&plan, Kind::Setup),
        ["solo"],
        "a shared team is written once"
    );
}

#[test]
fn existing_world_reused() {
    let (dir, mut p) = empty_project();
    let world =
        ItemBody::parse(Kind::World, "w", "format: shakedown-world/3\nlabel: Mine\n").unwrap();
    p.put(Kind::World, &Id::parse("toybench").unwrap(), world)
        .unwrap();
    let plan = plan(&p, &benches(), &[]);
    assert!(ids(&plan, Kind::World).is_empty());
    assert!(Path::new(&dir.path().join("worlds/toybench/world.yaml")).is_file());
    assert!(
        plan.skipped
            .iter()
            .any(|s| s.contains("toybench already exists")),
        "{:?}",
        plan.skipped
    );
}
