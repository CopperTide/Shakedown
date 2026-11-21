//! Deploying a project experiment: its files become an experiment on the engine, which remembers
//! the file it came from, and the file experiment lists its deployments.

mod support;
use support::records;

use shaked::{
    model::{Runner, Team},
    store::{NewExperiment, NewSetup},
};
use support::{packages::toy_world, run, AppFx};

fn team() -> Team {
    let agent = serde_json::json!({"id": "planner", "name": "Planner", "kind": "open", "model": "m", "prompt": "Poke the box."});
    serde_json::from_value(serde_json::json!({"id": "t", "name": "Team t", "agents": [agent]}))
        .unwrap()
}

/// Stores a one-setup experiment on the toy benchmark, deployed from `project_experiment`.
async fn store_one(fx: &AppFx, project_experiment: Option<&str>) -> String {
    let (t, scenario) = (team(), Default::default());
    let setups = [NewSetup {
        name: "A",
        team: &t,
        runner: Runner::Chain,
        connection: None,
    }];
    let new = NewExperiment {
        name: "From a file",
        task: "t1",
        runs: 1,
        benchmark: "toybench",
        scenario: &scenario,
        setups: &setups,
        max_cost_usd: None,
        seed: 7,
        world_minutes: None,
        project_experiment,
    };
    records(&fx.state.store)
        .create_experiment(new)
        .await
        .unwrap()
        .0
        .id
}

#[test]
fn create_experiment_records_its_file() {
    run(async {
        let root = toy_world("deploy-store", &[], &["--scored"]);
        let fx = AppFx::idle("deploy", "store")
            .root(root)
            .bench("toybench")
            .start()
            .await;
        let id = store_one(&fx, Some("slip")).await;
        store_one(&fx, None).await;
        let found = records(&fx.state.store)
            .deployments_of("slip")
            .await
            .unwrap();
        assert_eq!(found, vec![id]);
        assert!(records(&fx.state.store)
            .deployments_of("other")
            .await
            .unwrap()
            .is_empty());
    });
}
