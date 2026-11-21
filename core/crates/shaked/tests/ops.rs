//! The operation registry: every operation is named, typed and runnable, and edits show a diff
//! before anything is written unless the project allows otherwise.

mod support;

use serde_json::{json, Value};
use shaked::ops::{self, Access};
use support::{run, AppFx};

fn larkspur() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    support::copy_dir(&support::project_fixture(), dir.path());
    dir
}

async fn call(fx: &AppFx, name: &str, input: Value) -> Result<Value, ops::OpError> {
    let op = ops::find(name).unwrap_or_else(|| panic!("no operation {name}"));
    (op.run)(&fx.state, op, input).await
}

fn scenario_body(label: &str) -> Value {
    json!({
        "format": "shakedown-scenario/1",
        "label": label,
        "world": "larkspur-plant",
        "task": "Keep SO-1001 on schedule.",
    })
}

#[test]
fn names_unique_and_dotted() {
    let mut seen = std::collections::HashSet::new();
    for op in ops::all() {
        let (area, verb) = op.name.split_once('.').expect("dotted");
        assert!(area.chars().all(|c| c.is_ascii_lowercase()), "{}", op.name);
        assert!(
            verb.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{}",
            op.name
        );
        assert!(seen.insert(op.name), "{} twice", op.name);
    }
    assert!(ops::find("scenarios.put").is_some());
}

#[test]
fn schemas_are_objects() {
    for op in ops::all() {
        let schema = (op.input_schema)();
        assert_eq!(
            schema["type"], "object",
            "{} input schema: {schema}",
            op.name
        );
    }
}

#[test]
fn access_classes_follow_the_verb() {
    for op in ops::all() {
        let want = match op.name.rsplit('.').next().unwrap() {
            "list" | "get" | "status" => Access::Read,
            "delete" => Access::Delete,
            _ => Access::Edit,
        };
        assert_eq!(op.access, want, "{}", op.name);
    }
}

#[test]
fn list_and_get() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("ops", "list").project(dir.path()).start().await;
        let listed = call(&fx, "setups.list", json!({})).await.unwrap();
        assert_eq!(listed["items"].as_array().unwrap().len(), 3);
        let got = call(&fx, "scenarios.get", json!({"id": "supplier-slip"}))
            .await
            .unwrap();
        assert_eq!(got["label"], "A key supplier slips a week");
        assert_eq!(got["kind"], "scenario");
        let missing = call(&fx, "scenarios.get", json!({"id": "nope"}))
            .await
            .unwrap_err();
        assert_eq!(missing.status.as_u16(), 404);
    });
}

#[test]
fn put_diff_first_does_not_write() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("ops", "diff").project(dir.path()).start().await;
        let path = dir.path().join("scenarios/supplier-slip.yaml");
        let before = std::fs::read_to_string(&path).unwrap();
        let input = json!({"id": "supplier-slip", "body": scenario_body("Renamed")});
        let out = call(&fx, "scenarios.put", input).await.unwrap();
        assert_eq!(out["proposed"], true);
        assert!(
            out["diff"].as_str().unwrap().contains("+label: Renamed"),
            "{out}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    });
}

#[test]
fn put_confirm_writes() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("ops", "confirm")
            .project(dir.path())
            .start()
            .await;
        let input = json!({"id": "new-one", "body": scenario_body("New"), "confirm": true});
        let out = call(&fx, "scenarios.put", input).await.unwrap();
        assert_eq!(out["label"], "New");
        assert!(dir.path().join("scenarios/new-one.yaml").is_file());
    });
}

#[test]
fn put_rejects_inline_secrets() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("ops", "secret")
            .project(dir.path())
            .start()
            .await;
        let body = json!({"format": "shakedown-setup/1", "label": "S",
                          "team": {"kind": "http", "url": "https://x", "auth": {"key": "sk-ant-zzz999"}}});
        let input = json!({"id": "s", "body": body, "confirm": true});
        let err = call(&fx, "setups.put", input).await.unwrap_err();
        assert_eq!(err.status.as_u16(), 422);
        assert!(!err.message.contains("zzz999"));
    });
}

#[test]
fn bad_input_is_400() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("ops", "bad").project(dir.path()).start().await;
        let err = call(&fx, "scenarios.get", json!({})).await.unwrap_err();
        assert_eq!(err.status.as_u16(), 400);
        let err = call(&fx, "scenarios.get", json!({"id": "../x"}))
            .await
            .unwrap_err();
        assert_eq!(err.status.as_u16(), 422);
    });
}

#[test]
fn delete_removes_file() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("ops", "delete")
            .project(dir.path())
            .start()
            .await;
        call(
            &fx,
            "experiments.delete",
            json!({"id": "supplier-slip-solo-vs-team"}),
        )
        .await
        .unwrap();
        assert!(!dir
            .path()
            .join("experiments/supplier-slip-solo-vs-team.yaml")
            .exists());
    });
}

#[test]
fn status_counts_and_problems() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("ops", "status")
            .project(dir.path())
            .start()
            .await;
        let status = call(&fx, "project.status", json!({})).await.unwrap();
        assert_eq!(status["name"], "Larkspur Ops");
        assert_eq!(status["counts"]["setups"], 3);
        assert_eq!(status["problems"].as_array().unwrap().len(), 0);
    });
}

#[test]
fn no_project_is_409() {
    run(async {
        let fx = AppFx::idle("ops", "none").start().await;
        let err = call(&fx, "worlds.list", json!({})).await.unwrap_err();
        assert_eq!(err.status.as_u16(), 409);
    });
}

#[test]
fn init_creates_a_project() {
    run(async {
        let dir = tempfile::tempdir().unwrap();
        let fx = AppFx::idle("ops", "init").project(dir.path()).start().await;
        call(&fx, "project.init", json!({"name": "Fresh", "git": false}))
            .await
            .unwrap();
        let status = call(&fx, "project.status", json!({})).await.unwrap();
        assert_eq!(status["name"], "Fresh");
        let again = call(&fx, "project.init", json!({"name": "Again", "git": false})).await;
        assert_eq!(again.unwrap_err().status.as_u16(), 409);
    });
}

#[test]
fn http_put_then_get_round_trips() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("ops", "http").project(dir.path()).start().await;
        let url = format!("{}/api/project/scenarios/from-http", fx.base);
        let body = json!({"body": scenario_body("Over HTTP"), "confirm": true});
        let put = fx.http.put(&url).json(&body).send().await.unwrap();
        assert_eq!(put.status().as_u16(), 200);
        let (status, got) = fx.get("/api/project/scenarios/from-http").await;
        assert_eq!(status, 200, "{got}");
        assert_eq!(got["label"], "Over HTTP");
        let (status, listed) = fx.get("/api/project/scenarios").await;
        assert_eq!(status, 200);
        assert_eq!(listed["items"].as_array().unwrap().len(), 2);
    });
}

#[test]
fn http_errors_keep_their_status() {
    run(async {
        let fx = AppFx::idle("ops", "http409").start().await;
        let (status, _) = fx.get("/api/project").await;
        assert_eq!(status, 409);
    });
}
