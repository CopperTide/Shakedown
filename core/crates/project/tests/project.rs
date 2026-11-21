//! A project folder: created, opened, validated across files, and written without ever leaving a
//! half-written file.

use std::path::Path;

use shakedown_project::format::{parse, ExperimentFile, Kind, ScenarioFile};
use shakedown_project::{Id, ItemBody, Project};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn larkspur() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/larkspur");
    copy_dir(&fixture, dir.path());
    dir
}

fn id(s: &str) -> Id {
    Id::parse(s).unwrap()
}

#[test]
fn init_creates_layout() {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::init(dir.path(), "Larkspur Ops", false).unwrap();
    assert_eq!(project.config.name, "Larkspur Ops");
    for kind in Kind::ALL {
        assert!(
            dir.path().join(kind.dir()).is_dir(),
            "{} missing",
            kind.dir()
        );
    }
    assert!(dir.path().join("shakedown.yaml").is_file());
    let ignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(ignore.contains(".shakedown/"));
    assert!(!dir.path().join(".git").exists());
}

#[test]
fn init_refuses_an_existing_project() {
    let dir = larkspur();
    assert!(Project::init(dir.path(), "Again", false).is_err());
}

#[test]
fn open_lists_fixture_items() {
    let dir = larkspur();
    let project = Project::open(dir.path()).unwrap();
    assert_eq!(project.list(Kind::World).len(), 1);
    assert_eq!(project.list(Kind::Scenario).len(), 1);
    assert_eq!(project.list(Kind::Setup).len(), 3);
    assert_eq!(project.list(Kind::Experiment).len(), 1);
    assert!(project.problems.is_empty(), "{:?}", project.problems);
}

#[test]
fn rejects_unsafe_ids() {
    let too_long = "a".repeat(65);
    for bad in ["../x", "a/b", "", "A", too_long.as_str(), "-x", "x.yaml"] {
        assert!(Id::parse(bad).is_err(), "{bad:?} was accepted");
    }
    assert!(Id::parse(&"a".repeat(64)).is_ok());
}

#[test]
fn write_is_atomic() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    let path = dir
        .path()
        .join("experiments/supplier-slip-solo-vs-team.yaml");
    let before = std::fs::read_to_string(&path).unwrap();
    let mut body = experiment(&project);
    body.budget.per_run_usd = Some(f64::NAN);
    let id = id("supplier-slip-solo-vs-team");
    assert!(project
        .put(Kind::Experiment, &id, ItemBody::Experiment(body))
        .is_err());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        before,
        "a failed write changed the file"
    );

    let mut body = experiment(&project);
    body.label = "Renamed".into();
    let item = project
        .put(Kind::Experiment, &id, ItemBody::Experiment(body))
        .unwrap();
    assert_eq!(item.label, "Renamed");
    let reread: ExperimentFile = parse("x", &std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(reread.label, "Renamed");
    assert_no_temp_files(&dir.path().join("experiments"));
}

fn experiment(project: &Project) -> ExperimentFile {
    match &project.list(Kind::Experiment)[0].body {
        ItemBody::Experiment(e) => e.clone(),
        other => panic!("expected an experiment, got {other:?}"),
    }
}

fn assert_no_temp_files(dir: &Path) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(!name.contains(".tmp"), "left a temp file: {name}");
    }
}

#[test]
fn put_refuses_a_body_of_another_kind() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    let body = ItemBody::Experiment(experiment(&project));
    assert!(project.put(Kind::Scenario, &id("x"), body).is_err());
}

#[test]
fn put_creates_a_new_world_folder() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    let world = project.list(Kind::World)[0].body.clone();
    project
        .put(Kind::World, &id("second-plant"), world)
        .unwrap();
    assert!(dir.path().join("worlds/second-plant/world.yaml").is_file());
    assert_eq!(project.list(Kind::World).len(), 2);
}

#[test]
fn dangling_reference_reported() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    project.remove(Kind::World, &id("larkspur-plant")).unwrap();
    assert_eq!(
        project.list(Kind::Scenario).len(),
        1,
        "the scenario still lists"
    );
    let problem = project
        .problems
        .iter()
        .find(|p| p.kind == Some(Kind::Scenario))
        .expect("a problem on the scenario");
    assert_eq!(problem.id.as_deref(), Some("supplier-slip"));
    assert_eq!(problem.pointer, "/world");
}

#[test]
fn experiment_world_must_match_its_scenario() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    let world = project.list(Kind::World)[0].body.clone();
    project.put(Kind::World, &id("other"), world).unwrap();
    let mut body = experiment(&project);
    body.world = "other".into();
    let exp = id("supplier-slip-solo-vs-team");
    project
        .put(Kind::Experiment, &exp, ItemBody::Experiment(body))
        .unwrap();
    assert!(project
        .problems
        .iter()
        .any(|p| p.kind == Some(Kind::Experiment) && p.pointer == "/world"));
}

#[test]
fn duplicate_world_dir_and_file() {
    let dir = larkspur();
    std::fs::write(
        dir.path().join("worlds/loose.yaml"),
        "format: shakedown-world/3\nlabel: L\n",
    )
    .unwrap();
    let project = Project::open(dir.path()).unwrap();
    assert_eq!(
        project.list(Kind::World).len(),
        1,
        "a loose world file is not a world"
    );
    assert!(project
        .problems
        .iter()
        .any(|p| p.message.contains("folder")));
}

#[test]
fn broken_file_is_a_problem_not_a_failure() {
    let dir = larkspur();
    std::fs::write(dir.path().join("scenarios/broken.yaml"), "format: [\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    assert_eq!(project.list(Kind::Scenario).len(), 1);
    let problem = project
        .problems
        .iter()
        .find(|p| p.id.as_deref() == Some("broken"));
    assert!(problem.is_some(), "{:?}", project.problems);
}

#[test]
fn remove_unknown_item_is_not_found() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    assert!(project.remove(Kind::Scenario, &id("nope")).is_err());
}

#[test]
fn scenario_hash_changes_with_content() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    let before = project.list(Kind::Scenario)[0].hash.clone();
    let mut body: ScenarioFile = match &project.list(Kind::Scenario)[0].body {
        ItemBody::Scenario(s) => s.clone(),
        _ => unreachable!(),
    };
    body.task.push_str(" Quickly.");
    let item = project
        .put(
            Kind::Scenario,
            &id("supplier-slip"),
            ItemBody::Scenario(body),
        )
        .unwrap();
    assert_ne!(item.hash, before);
}

#[test]
fn set_config_round_trips() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    let mut config = project.config.clone();
    config.name = "Renamed Ops".into();
    project.set_config(config.clone()).unwrap();
    assert_eq!(Project::open(dir.path()).unwrap().config, config);
}
