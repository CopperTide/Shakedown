//! MADL setups: validated with the MADL crate and drawn as agents and connections. Any team works;
//! a team described in MADL gets the graph and the checks.

mod support;

use serde_json::{json, Value};
use shaked::ops::{self, OpCtx};
use support::{run, AppFx};

fn larkspur() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    support::copy_dir(&support::project_fixture(), dir.path());
    dir
}

async fn madl(fx: &AppFx, id: &str) -> Result<Value, ops::OpError> {
    let op = ops::find("setups.madl").expect("setups.madl");
    (op.run)(&OpCtx::of(&fx.state), op, json!({"id": id})).await
}

#[test]
fn valid_madl_graph() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("madl", "valid")
            .project(dir.path())
            .start()
            .await;
        let out = madl(&fx, "planning-team").await.unwrap();
        assert_eq!(out["valid"], true, "{out}");
        let agents: Vec<&str> = out["graph"]["agents"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|a| a["id"].as_str())
            .collect();
        assert_eq!(agents, ["researcher", "writer", "editor"]);
        let edges = out["graph"]["edges"].as_array().unwrap();
        assert!(
            edges
                .iter()
                .any(|e| e["from"] == "researcher" && e["to"] == "writer"),
            "{out}"
        );
    });
}

#[test]
fn invalid_madl_lists_codes() {
    run(async {
        let dir = larkspur();
        let file = dir.path().join("setups/planning-team/team.madl.yaml");
        let text = std::fs::read_to_string(&file).unwrap().replace(
            "members: [researcher, writer, editor]",
            "members: [researcher, ghost, editor]",
        );
        std::fs::write(&file, text).unwrap();
        let fx = AppFx::idle("madl", "invalid")
            .project(dir.path())
            .start()
            .await;
        let out = madl(&fx, "planning-team").await.unwrap();
        assert_eq!(out["valid"], false, "{out}");
        let first = &out["diagnostics"][0];
        assert!(
            first["code"].as_str().is_some_and(|c| !c.is_empty()),
            "{out}"
        );
        assert!(first["pointer"].as_str().unwrap().starts_with('/'), "{out}");
    });
}

#[test]
fn missing_madl_file() {
    run(async {
        let dir = larkspur();
        std::fs::remove_file(dir.path().join("setups/planning-team/team.madl.yaml")).unwrap();
        let fx = AppFx::idle("madl", "missing")
            .project(dir.path())
            .start()
            .await;
        let err = madl(&fx, "planning-team").await.unwrap_err();
        assert_eq!(err.status.as_u16(), 422);
        assert!(
            err.message
                .contains("team.madl.yaml is missing from the setup's folder"),
            "{}",
            err.message
        );
    });
}

#[test]
fn not_madl() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("madl", "not").project(dir.path()).start().await;
        let err = madl(&fx, "solo-planner").await.unwrap_err();
        assert_eq!(err.status.as_u16(), 422);
        assert!(
            err.message.contains("isn't described in MADL"),
            "{}",
            err.message
        );
    });
}
