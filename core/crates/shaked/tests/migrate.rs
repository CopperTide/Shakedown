//! Moving earlier work into the project: benchmarks become worlds, earlier experiments become
//! files, and the old record becomes the file's first deployment. Running it twice changes nothing.

mod support;
use support::records;

use serde_json::{json, Value};
use shaked::{
    model::{Runner, Team},
    store::{NewExperiment, NewSetup},
};
use support::{packages::toy_world, run, AppFx};

fn team() -> Team {
    let agent = json!({"id": "planner", "name": "Planner", "kind": "open", "model": "m", "prompt": "Poke the box."});
    serde_json::from_value(json!({"id": "poker", "name": "Poker", "agents": [agent]})).unwrap()
}

/// An earlier experiment on the toy benchmark: its id.
async fn earlier(fx: &AppFx) -> String {
    let (t, scenario) = (team(), Default::default());
    let setups = [NewSetup {
        name: "A",
        team: &t,
        runner: Runner::Chain,
        connection: None,
    }];
    let new = NewExperiment {
        name: "Old box poke",
        task: "t1",
        runs: 2,
        benchmark: "toybench",
        scenario: &scenario,
        setups: &setups,
        max_cost_usd: None,
        seed: 7,
        world_minutes: None,
        project_experiment: None,
    };
    records(&fx.state.store)
        .create_experiment(new)
        .await
        .unwrap()
        .0
        .id
}

async fn import(fx: &AppFx, confirm: bool) -> Value {
    let (status, out) = fx
        .post("/api/project/import", json!({"confirm": confirm}))
        .await;
    assert_eq!(status, 200, "{out}");
    out
}

async fn app(tag: &str, dir: &std::path::Path) -> AppFx {
    let root = toy_world(tag, &[], &["--scored"]);
    AppFx::idle("migrate", tag)
        .root(root)
        .bench("toybench")
        .project(dir)
        .start()
        .await
}

#[test]
fn import_proposes_then_writes_and_links() {
    run(async {
        let dir = tempfile::tempdir().unwrap();
        shakedown_project::Project::init(dir.path(), "Fresh", false).unwrap();
        let fx = app("migrate-write", dir.path()).await;
        let old = earlier(&fx).await;
        let proposed = import(&fx, false).await;
        assert_eq!(proposed["proposed"], true, "{proposed}");
        assert!(
            !dir.path().join("worlds/toybench").exists(),
            "nothing written before confirming"
        );
        let done = import(&fx, true).await;
        assert!(
            dir.path().join("worlds/toybench/world.yaml").is_file(),
            "{done}"
        );
        assert!(dir.path().join("setups/poker.yaml").is_file(), "{done}");
        let exp = done["written"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["kind"] == "experiment")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let linked = records(&fx.state.store).deployments_of(&exp).await.unwrap();
        assert_eq!(linked, vec![old]);
    });
}

#[test]
fn import_is_idempotent() {
    run(async {
        let dir = tempfile::tempdir().unwrap();
        shakedown_project::Project::init(dir.path(), "Fresh", false).unwrap();
        let fx = app("migrate-twice", dir.path()).await;
        earlier(&fx).await;
        import(&fx, true).await;
        let again = import(&fx, true).await;
        assert_eq!(again["written"].as_array().unwrap().len(), 0, "{again}");
        assert_eq!(again["linked"], 0, "{again}");
    });
}
