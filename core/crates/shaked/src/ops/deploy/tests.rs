//! Mapping files onto the engine, on a copy of the example project.

use std::path::Path;

use shakedown_project::{Id, Project};

use super::request;

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy(&e.path(), &target)
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// The example project, with `edit(dir)` applied before it is opened.
fn project(edit: impl FnOnce(&Path)) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../project/tests/fixtures/larkspur"),
        dir.path(),
    );
    edit(dir.path());
    let project = Project::open(dir.path()).unwrap();
    (dir, project)
}

/// Adds `line` after the first line of `file`.
fn add_line(dir: &Path, file: &str, line: &str) {
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    let (first, rest) = text.split_once('\n').unwrap();
    std::fs::write(path, format!("{first}\n{line}\n{rest}")).unwrap();
}

fn runnable(dir: &Path) {
    add_line(dir, "worlds/larkspur-plant/world.yaml", "package: toybench");
    add_line(dir, "scenarios/supplier-slip.yaml", "preset: t1");
    let exp = dir.join("experiments/supplier-slip-solo-vs-team.yaml");
    let text = std::fs::read_to_string(&exp).unwrap().replace(
        "[solo-planner, planning-team, planner-with-rules]",
        "[solo-planner, planner-with-rules]",
    );
    std::fs::write(exp, text).unwrap();
}

fn exp() -> Id {
    Id::parse("supplier-slip-solo-vs-team").unwrap()
}

#[test]
fn maps_builtin_setup() {
    let (_dir, p) = project(runnable);
    let req = request(&p, &exp()).unwrap();
    assert_eq!(req.benchmark.as_deref(), Some("toybench"));
    assert_eq!(req.task, "t1");
    assert_eq!(req.runs, 5);
    assert_eq!(req.max_cost_usd, Some(1.5));
    assert_eq!(
        req.project_experiment.as_deref(),
        Some("supplier-slip-solo-vs-team")
    );
    let solo = &req.setups[0].team.agents[0];
    assert_eq!(
        (solo.model.as_deref(), solo.prompt.as_str()),
        (Some("claude-sonnet"), "You keep orders on schedule.")
    );
    assert_eq!(
        req.setups[1].team.agents[0].url.as_deref(),
        Some("https://agents.larkspur.internal/planner")
    );
}

#[test]
fn faults_become_disruptions() {
    let (_dir, p) = project(runnable);
    let scenario = request(&p, &exp())
        .unwrap()
        .scenario
        .expect("a fault was declared");
    assert_eq!(scenario.disruptions.len(), 1);
    assert_eq!(scenario.disruptions[0].id, "pool_exhausted");
    assert_eq!(scenario.disruptions[0].params["free"], 0);
}

#[test]
fn no_package_is_explained() {
    let (_dir, p) = project(|dir| {
        runnable(dir);
        let path = dir.join("worlds/larkspur-plant/world.yaml");
        std::fs::write(
            &path,
            std::fs::read_to_string(&path)
                .unwrap()
                .replace("package: toybench\n", ""),
        )
        .unwrap();
    });
    let err = request(&p, &exp()).unwrap_err();
    assert_eq!(err.status.as_u16(), 422);
    assert!(
        err.message
            .contains("Larkspur Valve Works has no runtime yet"),
        "{}",
        err.message
    );
}

#[test]
fn no_preset_is_explained() {
    let (_dir, p) =
        project(|dir| add_line(dir, "worlds/larkspur-plant/world.yaml", "package: toybench"));
    let err = request(&p, &exp()).unwrap_err();
    assert!(err.message.contains("set preset:"), "{}", err.message);
}

#[test]
fn code_and_madl_setups_refused() {
    let (_dir, p) = project(|dir| {
        runnable(dir);
        let exp = dir.join("experiments/supplier-slip-solo-vs-team.yaml");
        std::fs::write(
            &exp,
            std::fs::read_to_string(&exp)
                .unwrap()
                .replace("[solo-planner, planner-with-rules]", "[planning-team]"),
        )
        .unwrap();
    });
    let err = request(&p, &exp()).unwrap_err();
    assert_eq!(err.status.as_u16(), 422);
    assert!(err.message.starts_with("planning-team:"), "{}", err.message);
}

#[test]
fn problems_block_deploy() {
    let (_dir, p) = project(|dir| {
        runnable(dir);
        let exp = dir.join("experiments/supplier-slip-solo-vs-team.yaml");
        std::fs::write(
            &exp,
            std::fs::read_to_string(&exp)
                .unwrap()
                .replace("[solo-planner, planner-with-rules]", "[solo-planner, gone]"),
        )
        .unwrap();
    });
    let err = request(&p, &exp()).unwrap_err();
    assert!(
        err.message
            .contains("fix supplier-slip-solo-vs-team before deploying"),
        "{}",
        err.message
    );
}
