//! A project folder: created, opened, validated across files, and written without ever leaving a
//! half-written file.

use std::path::Path;

use shakedown_project::format::{parse, ExperimentFile, Kind, ScenarioFile, WorldFile};
use shakedown_project::{Fate, Id, ItemBody, Project, ProjectError};

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

/// Adds experiment `name`, comparing `setups` in the fixture's scenario.
fn put_experiment(project: &mut Project, name: &str, setups: &[&str]) {
    let mut body = experiment(project);
    body.label = name.into();
    body.setups = setups.iter().map(|s| s.to_string()).collect();
    let body = ItemBody::Experiment(body);
    project.put(Kind::Experiment, &id(name), body).unwrap();
}

/// Adds world `name` in `group`.
fn put_world(project: &mut Project, name: &str, group: Option<&str>) {
    let mut body: WorldFile = project
        .body::<WorldFile>(&id("larkspur-plant"))
        .unwrap()
        .clone();
    body.label = name.into();
    body.group = group.map(str::to_string);
    project
        .put(Kind::World, &id(name), ItemBody::World(body))
        .unwrap();
}

fn experiment_of(s: &str) -> (Kind, Id) {
    (Kind::Experiment, id(s))
}

#[test]
fn dependents_follow_references() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    put_experiment(&mut project, "solo-only", &["solo-planner"]);
    let both = vec![
        experiment_of("solo-only"),
        experiment_of("supplier-slip-solo-vs-team"),
    ];
    let mut world = vec![(Kind::Scenario, id("supplier-slip"))];
    world.extend(both.clone());
    assert_eq!(
        project.dependents(Kind::World, &id("larkspur-plant")),
        world
    );
    assert_eq!(
        project.dependents(Kind::Scenario, &id("supplier-slip")),
        both
    );
    assert_eq!(project.dependents(Kind::Setup, &id("solo-planner")), both);
    assert_eq!(
        project.dependents(Kind::Setup, &id("planning-team")),
        vec![experiment_of("supplier-slip-solo-vs-team")]
    );
    assert!(project
        .dependents(Kind::Experiment, &id("solo-only"))
        .is_empty());
}

#[test]
fn a_world_reaches_experiments_through_its_scenarios() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    put_world(&mut project, "other", None);
    let mut body = experiment(&project);
    body.world = "other".into();
    let exp = id("supplier-slip-solo-vs-team");
    project
        .put(Kind::Experiment, &exp, ItemBody::Experiment(body))
        .unwrap();
    let mine = project.dependents(Kind::World, &id("larkspur-plant"));
    assert!(mine.contains(&(Kind::Experiment, exp.clone())), "{mine:?}");
    let other = project.dependents(Kind::World, &id("other"));
    assert_eq!(other, vec![(Kind::Experiment, exp)]);
}

#[test]
fn delete_refuses_while_used() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    let err = project
        .delete(Kind::Scenario, &id("supplier-slip"), false, None)
        .unwrap_err();
    match &err {
        ProjectError::InUse { by, .. } => {
            assert_eq!(by.len(), 1);
            assert_eq!(by[0].id, id("supplier-slip-solo-vs-team"));
            assert_eq!(by[0].label, "Supplier slip: solo vs team");
            assert_eq!(by[0].fates, [Fate::Deleted]);
        }
        other => panic!("expected InUse, got {other:?}"),
    }
    assert!(
        err.to_string().contains("supplier-slip-solo-vs-team"),
        "{err}"
    );
    assert!(dir.path().join("scenarios/supplier-slip.yaml").is_file());
}

#[test]
fn an_unused_item_deletes_without_cascade() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    let exp = id("supplier-slip-solo-vs-team");
    let removal = project.delete(Kind::Experiment, &exp, false, None).unwrap();
    assert!(removal.fallout.is_empty());
    assert!(project.get(Kind::Experiment, &exp).is_none());
    assert!(project.delete(Kind::Experiment, &exp, false, None).is_err());
}

#[test]
fn cascade_deletes_what_uses_a_world_and_keeps_its_data() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    std::fs::write(
        dir.path().join("worlds/larkspur-plant/suppliers.csv"),
        "id\n",
    )
    .unwrap();
    let fallout = project
        .delete(Kind::World, &id("larkspur-plant"), true, None)
        .unwrap()
        .fallout;
    assert_eq!(fallout.len(), 2);
    assert!(fallout.iter().all(|d| d.deleted()));
    for kind in [Kind::World, Kind::Scenario, Kind::Experiment] {
        assert!(project.list(kind).is_empty(), "{kind:?} left");
    }
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    assert!(!dir.path().join("worlds/larkspur-plant/world.yaml").exists());
    assert!(dir
        .path()
        .join("worlds/larkspur-plant/suppliers.csv")
        .is_file());
    assert_eq!(
        project.list(Kind::Setup).len(),
        3,
        "setups are not the world's"
    );
}

#[test]
fn deleting_a_setup_drops_it_from_experiments() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    put_experiment(&mut project, "solo-only", &["solo-planner"]);
    let fallout = project
        .delete(Kind::Setup, &id("solo-planner"), true, None)
        .unwrap()
        .fallout;
    let fates: Vec<(String, Fate)> = fallout
        .iter()
        .map(|d| (d.id.to_string(), d.fates[0]))
        .collect();
    assert_eq!(
        fates,
        vec![
            ("solo-only".to_string(), Fate::Deleted),
            ("supplier-slip-solo-vs-team".to_string(), Fate::SetupDropped),
        ]
    );
    assert!(project.get(Kind::Experiment, &id("solo-only")).is_none());
    let kept = project
        .body::<ExperimentFile>(&id("supplier-slip-solo-vs-team"))
        .unwrap();
    assert_eq!(kept.setups, vec!["planning-team", "planner-with-rules"]);
    assert!(project.get(Kind::Setup, &id("solo-planner")).is_none());
    assert!(project.problems.is_empty(), "{:?}", project.problems);
}

#[test]
fn rename_group_moves_every_world_in_it() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    put_world(&mut project, "acme-east", Some("Acme"));
    put_world(&mut project, "acme-west", Some("Acme"));
    put_world(&mut project, "bolt", Some("Bolt"));
    let bolt = project.get(Kind::World, &id("bolt")).unwrap().hash.clone();
    let moved = project.rename_group("Acme", Some("Acme Foods")).unwrap();
    assert_eq!(moved, vec![id("acme-east"), id("acme-west")]);
    let group = |p: &Project, w: &str| p.body::<WorldFile>(&id(w)).unwrap().group.clone();
    assert_eq!(group(&project, "acme-east").as_deref(), Some("Acme Foods"));
    assert_eq!(group(&project, "acme-west").as_deref(), Some("Acme Foods"));
    assert_eq!(project.get(Kind::World, &id("bolt")).unwrap().hash, bolt);
    project.rename_group("Acme Foods", None).unwrap();
    assert_eq!(group(&project, "acme-east"), None);
    let text = std::fs::read_to_string(dir.path().join("worlds/acme-east/world.yaml")).unwrap();
    assert!(!text.contains("group"), "{text}");
    assert!(project.rename_group("Nobody", Some("X")).is_err());
}

/// Adds experiment `name` in world `world` and scenario `scenario`, branched from `parent`.
fn put_branch(project: &mut Project, name: &str, at: (&str, &str), parent: &str) {
    let mut body = experiment(project);
    (body.world, body.scenario) = (at.0.into(), at.1.into());
    body.parent = Some(parent.into());
    project
        .put(Kind::Experiment, &id(name), ItemBody::Experiment(body))
        .unwrap();
}

/// Adds world `other` and its scenario `other-slip`.
fn put_other_world(project: &mut Project) {
    put_world(project, "other", None);
    let mut body = project
        .body::<ScenarioFile>(&id("supplier-slip"))
        .unwrap()
        .clone();
    body.world = "other".into();
    project
        .put(Kind::Scenario, &id("other-slip"), ItemBody::Scenario(body))
        .unwrap();
}

const PARENT: &str = "supplier-slip-solo-vs-team";

#[test]
fn sub_experiments_lose_their_parent() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    put_branch(
        &mut project,
        "branch",
        ("larkspur-plant", "supplier-slip"),
        PARENT,
    );
    assert_eq!(
        project.dependents(Kind::Experiment, &id(PARENT)),
        vec![experiment_of("branch")]
    );
    let refused = project.delete(Kind::Experiment, &id(PARENT), false, None);
    assert!(matches!(refused, Err(ProjectError::InUse { .. })));
    let fallout = project
        .delete(Kind::Experiment, &id(PARENT), true, None)
        .unwrap()
        .fallout;
    assert_eq!(fallout[0].fates, [Fate::ParentCleared]);
    assert_eq!(
        project
            .body::<ExperimentFile>(&id("branch"))
            .unwrap()
            .parent,
        None
    );
    assert!(project.problems.is_empty(), "{:?}", project.problems);
}

#[test]
fn a_cascade_frees_sub_experiments_elsewhere() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    put_other_world(&mut project);
    put_branch(&mut project, "elsewhere", ("other", "other-slip"), PARENT);
    let fallout = project
        .delete(Kind::Scenario, &id("supplier-slip"), true, None)
        .unwrap()
        .fallout;
    let fates: Vec<(String, Fate)> = fallout
        .iter()
        .map(|d| (d.id.to_string(), d.fates[0]))
        .collect();
    let want = [
        ("elsewhere".to_string(), Fate::ParentCleared),
        (PARENT.to_string(), Fate::Deleted),
    ];
    assert_eq!(fates, want);
    assert!(project.problems.is_empty(), "{:?}", project.problems);
}

#[test]
fn a_stale_preview_deletes_nothing() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    let shown: Vec<_> = project
        .fallout(Kind::Setup, &id("solo-planner"))
        .into_iter()
        .map(|d| (d.kind, d.id, d.fates))
        .collect();
    put_experiment(&mut project, "solo-only", &["solo-planner"]);
    let err = project
        .delete(Kind::Setup, &id("solo-planner"), true, Some(&shown))
        .unwrap_err();
    match err {
        ProjectError::Stale { by, .. } => assert_eq!(by.len(), 2, "the fresh dependents: {by:?}"),
        other => panic!("expected Stale, got {other:?}"),
    }
    assert!(project.get(Kind::Experiment, &id("solo-only")).is_some());
    let now: Vec<_> = project
        .fallout(Kind::Setup, &id("solo-planner"))
        .into_iter()
        .map(|d| (d.kind, d.id, d.fates))
        .collect();
    assert!(project
        .delete(Kind::Setup, &id("solo-planner"), true, Some(&now))
        .is_ok());
}

#[test]
fn a_delete_goes_to_the_trash_and_comes_back() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    put_experiment(&mut project, "solo-only", &["solo-planner"]);
    let parent_file = dir.path().join(format!("experiments/{PARENT}.yaml"));
    let before = std::fs::read_to_string(&parent_file).unwrap();
    let removal = project
        .delete(Kind::Setup, &id("solo-planner"), true, None)
        .unwrap();
    let bin = dir.path().join(".shakedown/trash").join(&removal.trash);
    assert!(bin.join("setups/solo-planner.yaml").is_file());
    assert!(bin.join("experiments/solo-only.yaml").is_file());
    assert!(
        bin.join(format!("experiments/{PARENT}.yaml")).is_file(),
        "the rewritten file's copy"
    );
    let manifest = project.restore(None).unwrap();
    assert_eq!(manifest.label, "setup solo-planner");
    assert_eq!(project.list(Kind::Setup).len(), 3);
    assert!(project.get(Kind::Experiment, &id("solo-only")).is_some());
    let after = std::fs::read_to_string(&parent_file).unwrap();
    assert_eq!(after, before, "the rewritten experiment is as it was");
    assert!(!bin.exists());
    assert!(project.restore(None).is_err(), "nothing left to undo");
}

#[test]
fn undo_refuses_to_overwrite_a_new_file() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    let file = dir.path().join(format!("experiments/{PARENT}.yaml"));
    let text = std::fs::read_to_string(&file).unwrap();
    project
        .delete(Kind::Experiment, &id(PARENT), false, None)
        .unwrap();
    std::fs::write(&file, text).unwrap();
    assert!(matches!(
        project.restore(None),
        Err(ProjectError::Occupied { .. })
    ));
    assert!(project.restore(Some("../x")).is_err());
}

#[test]
fn a_delete_that_cannot_reach_the_trash_changes_nothing() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    std::fs::create_dir_all(dir.path().join(".shakedown")).unwrap();
    std::fs::write(dir.path().join(".shakedown/trash"), "not a folder").unwrap();
    assert!(project
        .delete(Kind::World, &id("larkspur-plant"), true, None)
        .is_err());
    assert!(project.problems.is_empty());
    assert_eq!(
        Project::open(dir.path())
            .unwrap()
            .list(Kind::Experiment)
            .len(),
        1
    );
}

#[test]
fn a_kept_folder_is_named() {
    let dir = larkspur();
    let project = Project::open(dir.path()).unwrap();
    assert_eq!(
        project
            .kept_folder(Kind::World, &id("larkspur-plant"))
            .as_deref(),
        Some("worlds/larkspur-plant/")
    );
    assert_eq!(
        project
            .kept_folder(Kind::Setup, &id("planning-team"))
            .as_deref(),
        Some("setups/planning-team/")
    );
    assert_eq!(project.kept_folder(Kind::Setup, &id("solo-planner")), None);
    assert_eq!(
        project.kept_folder(Kind::Scenario, &id("supplier-slip")),
        None
    );
}

#[test]
fn an_experiment_can_lose_a_setup_and_its_parent_at_once() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    put_experiment(&mut project, "solo-only", &["solo-planner"]);
    let mut body = experiment(&project);
    body.setups = vec!["solo-planner".into(), "planning-team".into()];
    body.parent = Some("solo-only".into());
    project
        .put(Kind::Experiment, &id("child"), ItemBody::Experiment(body))
        .unwrap();
    let fallout = project.fallout(Kind::Setup, &id("solo-planner"));
    let child = fallout.iter().find(|d| d.id == id("child")).unwrap();
    assert_eq!(child.fates, [Fate::SetupDropped, Fate::ParentCleared]);
    project
        .delete(Kind::Setup, &id("solo-planner"), true, None)
        .unwrap();
    let after = project.body::<ExperimentFile>(&id("child")).unwrap();
    assert_eq!(
        (after.setups.clone(), after.parent.clone()),
        (vec!["planning-team".to_string()], None)
    );
    assert!(project.problems.is_empty(), "{:?}", project.problems);
}

#[test]
fn undo_refuses_to_overwrite_a_later_edit() {
    let dir = larkspur();
    let mut project = Project::open(dir.path()).unwrap();
    project
        .delete(Kind::Setup, &id("solo-planner"), true, None)
        .unwrap();
    let mut body = experiment(&project);
    body.label = "Edited after the delete".into();
    project
        .put(Kind::Experiment, &id(PARENT), ItemBody::Experiment(body))
        .unwrap();
    match project.restore(None) {
        Err(ProjectError::EditedSince { files }) => {
            assert_eq!(files, [format!("experiments/{PARENT}.yaml")])
        }
        other => panic!("expected EditedSince, got {other:?}"),
    }
    assert_eq!(experiment(&project).label, "Edited after the delete");
    assert!(project.get(Kind::Setup, &id("solo-planner")).is_none());
}

#[test]
fn the_trash_keeps_the_last_twenty_and_a_month() {
    let dir = larkspur();
    let trash = dir.path().join(".shakedown/trash");
    for n in 0..25 {
        std::fs::create_dir_all(trash.join(format!("{:013}", 1000 + n))).unwrap();
    }
    std::fs::create_dir_all(trash.join("9999999999999")).unwrap();
    let mut project = Project::open(dir.path()).unwrap();
    project
        .delete(Kind::Experiment, &id(PARENT), false, None)
        .unwrap();
    let left = std::fs::read_dir(&trash).unwrap().count();
    assert_eq!(
        left, 21,
        "the 20 newest old ones (one of them recent) and this delete"
    );
    assert!(
        trash.join("9999999999999").is_dir(),
        "a recent delete is kept"
    );
    assert!(!trash.join(format!("{:013}", 1000)).exists());
    let ignore = std::fs::read_to_string(dir.path().join(".shakedown/.gitignore")).unwrap();
    assert_eq!(ignore, "*\n");
}
