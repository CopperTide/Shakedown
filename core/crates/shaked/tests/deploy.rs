//! Deploying a project experiment: its files become an experiment on the engine, which remembers
//! the file it came from, and the file experiment lists its deployments.

mod support;
use support::records;

use serde_json::{json, Value};
use shaked::{
    model::{Runner, Team},
    store::{NewExperiment, NewSetup},
};
use shaked::{
    ops::{mcp, OpCtx},
    project_state::ProjectState,
    providers::ProviderConfig,
};
use support::{app, packages::toy_world, run, AppFx};

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

/// A provider that accepts connections and never answers: a run on it stays running.
async fn silent_provider() -> String {
    let (listener, base) = app::bind().await;
    tokio::spawn(async move {
        let mut held = vec![];
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    base
}

/// The example project made deployable on the toy benchmark: its world runs on `toybench`, its
/// scenario plays task t1 without faults, and its experiment compares the solo planner alone.
fn runnable_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    support::copy_dir(&support::project_fixture(), dir.path());
    let edit = |file: &str, from: &str, to: &str| {
        let path = dir.path().join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} lacks {from}");
        std::fs::write(path, text.replacen(from, to, 1)).unwrap();
    };
    edit(
        "worlds/larkspur-plant/world.yaml",
        "label: Larkspur Valve Works\n",
        "label: Larkspur Valve Works\npackage: toybench\n",
    );
    edit(
        "scenarios/supplier-slip.yaml",
        "world: larkspur-plant\n",
        "world: larkspur-plant\npreset: t1\n",
    );
    edit(
        "scenarios/supplier-slip.yaml",
        "db: larkspur-prod, fault: pool_exhausted, ",
        "",
    );
    edit(
        "experiments/supplier-slip-solo-vs-team.yaml",
        "[solo-planner, planning-team, planner-with-rules]",
        "[solo-planner]",
    );
    dir
}

async fn deploy_app(tag: &str, project: &std::path::Path) -> AppFx {
    let root = toy_world(tag, &[], &["--scored", "--two-tasks"]);
    AppFx::idle("deploy", tag)
        .root(root)
        .bench("toybench")
        .providers(ProviderConfig::with_upstream(&silent_provider().await))
        .project(project)
        .start()
        .await
}

const DEPLOY: &str = "/api/project/experiments/supplier-slip-solo-vs-team/deploy";
const RUNS: &str = "/api/project/experiments/supplier-slip-solo-vs-team/runs";

#[test]
fn deploy_starts_an_experiment_and_lists_it() {
    run(async {
        let dir = runnable_project();
        let fx = deploy_app("deploy-http", dir.path()).await;
        let (status, detail) = fx.post(DEPLOY, json!({})).await;
        assert_eq!(status, 200, "{detail}");
        assert_eq!(detail["name"], "Supplier slip: solo vs team");
        let (status, runs) = fx.get(RUNS).await;
        assert_eq!(status, 200, "{runs}");
        let deployments = runs["deployments"].as_array().unwrap();
        assert_eq!(deployments.len(), 1);
        assert_eq!(deployments[0]["id"], detail["id"]);
        assert_eq!(deployments[0]["setups"].as_array().unwrap().len(), 1);
    });
}

#[test]
fn read_token_cannot_deploy() {
    run(async {
        let dir = runnable_project();
        let fx = deploy_app("deploy-scope", dir.path()).await;
        let (_, minted) = fx
            .post("/api/tokens", json!({"name": "r", "scopes": ["read"]}))
            .await;
        let token = minted["token"].as_str().unwrap().to_string();
        let url = |p: &str| format!("{}{p}", fx.base);
        let refused = fx
            .http
            .post(url(DEPLOY))
            .bearer_auth(&token)
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(refused.status().as_u16(), 403);
        let listed = fx
            .http
            .get(url(RUNS))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(listed.status().as_u16(), 200);
    });
}

#[test]
fn deploy_over_stdio_needs_shaked() {
    run(async {
        let dir = runnable_project();
        let project = ProjectState::new(Some(dir.path().to_path_buf()));
        let ctx = OpCtx::project_only(std::sync::Arc::new(project));
        let req = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                         "params": {"name": "experiments_deploy", "arguments": {"id": "supplier-slip-solo-vs-team"}}});
        let reply: Value = mcp::handle(&ctx, &mcp::Asker::Local, &req).await.unwrap();
        assert_eq!(reply["result"]["isError"], true, "{reply}");
        let text = reply["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("needs a running shaked"), "{text}");
    });
}
