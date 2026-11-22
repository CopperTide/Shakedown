//! Mapping files onto the engine, on a copy of the example project.

use std::path::Path;

use shakedown_project::{Id, Project};

use crate::ops::OpError;

use super::{request as deploy_request, Credentials};
use crate::model::AuthRef;

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

/// Both names of the example project are usable.
fn creds() -> Credentials {
    let mut c = Credentials::default();
    c.provider("anthropic-main", Ok(()));
    c.credential(AuthRef::Key("planner-endpoint".into()), Ok(()));
    c
}

fn request(p: &Project, id: &Id) -> Result<crate::api::experiments::NewExperimentReq, OpError> {
    deploy_request(p, id, &creds())
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

#[test]
fn a_builtin_agents_key_becomes_its_provider() {
    let (_dir, p) = project(runnable);
    let req = request(&p, &exp()).unwrap();
    let solo = &req.setups[0].team.agents[0];
    assert_eq!(solo.provider.as_deref(), Some("key:anthropic-main"));
    assert_eq!(solo.model.as_deref(), Some("claude-sonnet"));
}

#[test]
fn a_key_that_is_not_stored_or_not_yours_is_refused() {
    let (_dir, p) = project(runnable);
    let err = deploy_request(&p, &exp(), &Credentials::default()).unwrap_err();
    assert_eq!(err.status.as_u16(), 400);
    assert!(
        err.message.contains("anthropic-main") && err.message.contains("solo-planner"),
        "{}",
        err.message
    );
    let mut withheld = creds();
    withheld.provider("anthropic-main", Err("is another member's".into()));
    let err = deploy_request(&p, &exp(), &withheld).unwrap_err();
    assert!(err.message.contains("another member's"), "{}", err.message);
}

#[test]
fn an_endpoints_auth_becomes_a_reference_without_the_secret() {
    let (_dir, p) = project(runnable);
    let req = request(&p, &exp()).unwrap();
    let endpoint = &req.setups[1].team.agents[0];
    assert_eq!(endpoint.auth, Some(AuthRef::Key("planner-endpoint".into())));
    assert!(endpoint.headers.is_empty());
}

#[test]
fn an_auth_that_may_not_be_used_is_refused_with_its_reason() {
    let (_dir, p) = project(runnable);
    let mut c = creds();
    c.credential(
        AuthRef::Key("planner-endpoint".into()),
        Err("the key is shared with you".into()),
    );
    let err = deploy_request(&p, &exp(), &c).unwrap_err();
    assert_eq!(err.status.as_u16(), 400);
    assert!(
        err.message.contains("planner-with-rules") && err.message.contains("shared with you"),
        "{}",
        err.message
    );
}

fn with_auth(auth: &'static str) -> (tempfile::TempDir, Project) {
    project(move |dir| {
        runnable(dir);
        let f = dir.join("setups/planner-with-rules.yaml");
        let text = std::fs::read_to_string(&f).unwrap();
        std::fs::write(&f, text.replace("auth: {key: planner-endpoint}", auth)).unwrap();
    })
}

#[test]
fn an_auth_naming_a_key_and_a_variable_is_refused() {
    let (_dir, p) = with_auth("auth: {key: planner-endpoint, env: SHAKEDOWN_AGENT_T}");
    let needs = super::needs(&p, &exp());
    assert_eq!(needs.err().map(|e| e.status.as_u16()), Some(400));
}

#[test]
fn an_auth_env_is_a_reference_to_the_variable() {
    let (_dir, p) = with_auth("auth: {env: SHAKEDOWN_AGENT_T}");
    let mut c = creds();
    c.credential(AuthRef::Env("SHAKEDOWN_AGENT_T".into()), Ok(()));
    let req = deploy_request(&p, &exp(), &c).unwrap();
    let got = &req.setups[1].team.agents[0].auth;
    assert_eq!(got, &Some(AuthRef::Env("SHAKEDOWN_AGENT_T".into())));
    let needs = super::needs(&p, &exp()).unwrap();
    assert_eq!(needs.auth, [AuthRef::Env("SHAKEDOWN_AGENT_T".into())]);
    assert_eq!(needs.providers, ["anthropic-main"]);
}

#[test]
fn problems_are_reported_before_any_key_is_looked_up() {
    let (_dir, p) = project(|dir| {
        runnable(dir);
        let exp = dir.join("experiments/supplier-slip-solo-vs-team.yaml");
        let text = std::fs::read_to_string(&exp).unwrap();
        let broken = text.replace("[solo-planner, planner-with-rules]", "[solo-planner, gone]");
        std::fs::write(&exp, broken).unwrap();
    });
    let err = super::needs(&p, &exp()).err().unwrap();
    assert!(
        err.message
            .contains("fix supplier-slip-solo-vs-team before deploying"),
        "{}",
        err.message
    );
}
