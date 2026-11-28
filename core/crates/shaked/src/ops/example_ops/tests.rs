//! Adding the example: what it writes, that it never overwrites, how its agent's model is picked,
//! and that the result deploys as a run of the built-in retail benchmark.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Value};
use shakedown_project::format::{Kind, SetupFile, Team};
use shakedown_project::{Id, Project};

use super::{pick, Connected, EXAMPLE};
use crate::ops::{deploy, find, OpCtx, OpError};
use crate::project_state::ProjectState;

fn folder() -> (tempfile::TempDir, Arc<ProjectState>) {
    let dir = tempfile::tempdir().unwrap();
    Project::init(dir.path(), "P", false).unwrap();
    let state = Arc::new(ProjectState::new(Some(dir.path().into())));
    (dir, state)
}

async fn call(state: &Arc<ProjectState>, args: Value) -> Result<Value, OpError> {
    let op = find("project.example").unwrap();
    (op.run)(&OpCtx::project_only(state.clone()), op, args).await
}

const MINE: &str = r#"{"key": "my-key", "model": "some/model", "confirm": true}"#;

fn id(s: &str) -> Id {
    Id::parse(s).unwrap()
}

#[tokio::test]
async fn without_confirm_it_answers_the_items_and_writes_nothing() {
    let (_dir, state) = folder();
    let out = call(&state, json!({"model": "m"})).await.unwrap();
    assert_eq!(out["proposed"], true);
    let kinds: Vec<&str> = (out["items"].as_array().unwrap().iter())
        .map(|i| i["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["world", "scenario", "setup", "experiment"]);
    assert_eq!(out["experiment"], EXAMPLE.items.experiment);
    let project = state.current().await.unwrap();
    assert!(Kind::ALL.iter().all(|k| project.list(*k).is_empty()));
}

#[tokio::test]
async fn it_writes_four_items_without_problems_on_your_key() {
    let (_dir, state) = folder();
    let out = call(&state, serde_json::from_str(MINE).unwrap())
        .await
        .unwrap();
    assert_eq!(out["written"].as_array().unwrap().len(), 4);
    assert_eq!(
        out["agent"],
        json!({"key": "my-key", "model": "some/model"})
    );
    let project = state.current().await.unwrap();
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    let setup = project
        .body::<SetupFile>(&id(&EXAMPLE.items.setup))
        .unwrap();
    let Team::Builtin { agents } = &setup.team else {
        panic!("{setup:?}")
    };
    assert!(agents
        .iter()
        .all(|a| a.key == "my-key" && a.model == "some/model"));
}

#[tokio::test]
async fn a_second_call_is_409_and_changes_nothing() {
    let (_dir, state) = folder();
    call(&state, serde_json::from_str(MINE).unwrap())
        .await
        .unwrap();
    let before = state.current().await.unwrap();
    let again = json!({"key": "other", "model": "other", "confirm": true});
    let err = call(&state, again).await.unwrap_err();
    assert_eq!(err.status, StatusCode::CONFLICT);
    assert_eq!(err.details.unwrap()["taken"].as_array().unwrap().len(), 4);
    let setup = id(&EXAMPLE.items.setup);
    let after = state.current().await.unwrap();
    assert_eq!(
        after.get(Kind::Setup, &setup),
        before.get(Kind::Setup, &setup)
    );
}

#[tokio::test]
async fn one_taken_id_is_409_and_nothing_is_written() {
    let (_dir, state) = folder();
    let world = json!({"format": "shakedown-world/3", "label": "Mine"});
    let put = find("worlds.put").unwrap();
    let args = json!({"id": EXAMPLE.items.world, "body": world, "confirm": true});
    (put.run)(&OpCtx::project_only(state.clone()), put, args)
        .await
        .unwrap();
    let err = call(&state, serde_json::from_str(MINE).unwrap())
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::CONFLICT);
    assert_eq!(
        err.details.unwrap()["taken"],
        json!([format!("world {}", EXAMPLE.items.world)])
    );
    let project = state.current().await.unwrap();
    assert!(project.list(Kind::Scenario).is_empty() && project.list(Kind::Setup).is_empty());
    assert_eq!(project.list(Kind::World)[0].label, "Mine");
}

#[test]
fn the_model_is_asked_else_the_keys_else_the_examples_for_its_provider() {
    let found = |model: Option<&str>, kind: Option<&str>| Connected {
        key: Some("k".into()),
        model: model.map(str::to_string),
        kind: kind.map(str::to_string),
    };
    let asked = pick(None, Some("a".into()), found(Some("b"), None)).unwrap();
    assert_eq!((asked.key.as_str(), asked.model.as_str()), ("k", "a"));
    assert_eq!(pick(None, None, found(Some("b"), None)).unwrap().model, "b");
    let by_kind = pick(None, None, found(None, Some("anthropic"))).unwrap();
    assert_eq!(by_kind.model, EXAMPLE.models["anthropic"]);
    let none = pick(None, None, found(None, Some("custom"))).unwrap_err();
    assert_eq!(none.status, StatusCode::UNPROCESSABLE_ENTITY);
    let default = pick(None, Some("m".into()), Connected::default()).unwrap();
    assert_eq!(default.key, "", "no key: the default provider");
}

#[tokio::test]
async fn the_example_deploys_as_a_retail_task_on_your_key() {
    let (_dir, state) = folder();
    call(&state, serde_json::from_str(MINE).unwrap())
        .await
        .unwrap();
    let project = state.current().await.unwrap();
    let exp = id(&EXAMPLE.items.experiment);
    let mut creds = deploy::Credentials::default();
    creds.provider("my-key", Ok(()));
    let req = deploy::request(&project, &exp, &creds, None).unwrap();
    assert_eq!(req.benchmark.as_deref(), Some("retail"));
    assert_eq!((req.runs, req.setups.len()), (1, 1));
    let agent = &req.setups[0].team.agents[0];
    assert_eq!(agent.provider.as_deref(), Some("key:my-key"));
    assert_eq!(agent.model.as_deref(), Some("some/model"));
}
