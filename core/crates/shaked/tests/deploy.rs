//! Deploying a project experiment: its files become an experiment on the engine, which remembers
//! the file it came from, and the file experiment lists its deployments.

mod support;
use support::records;

use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
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
use std::sync::{Arc, Mutex};
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

const VAULT_KEY: &str = "0001020304050607080910111213141516171819202122232425262728293031";

async fn deploy_app(tag: &str, project: &std::path::Path) -> AppFx {
    deploy_app_on(tag, project, None).await
}

async fn deploy_app_on(
    tag: &str,
    project: &std::path::Path,
    layer: Option<Arc<dyn shaked::layer::Layer>>,
) -> AppFx {
    let root = toy_world(tag, &[], &["--scored", "--two-tasks"]);
    let mut setup = AppFx::idle("deploy", tag)
        .root(root)
        .bench("toybench")
        .providers(ProviderConfig::with_upstream(&silent_provider().await))
        .project(project);
    if let Some(layer) = layer {
        setup = setup.layer(layer);
    }
    let fx = setup.start().await;
    let vault = shaked::vault::Vault::from_hex(VAULT_KEY).unwrap();
    let _ = fx.state.vault.set(Arc::new(vault));
    fx
}

/// Stores a key of `kind` named `name` that keys `base` (a server of this test), with `secret`.
async fn store_key(fx: &AppFx, name: &str, kind: &str, base: &str, secret: &str) {
    let body = json!({"name": name, "kind": kind, "secret": secret, "base_url": base});
    let (status, made) = fx.post("/api/models/keys", body).await;
    assert_eq!(status, 200, "{made}");
}

/// A model server that counts the connections made to it and never answers.
async fn counting_provider() -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (listener, base) = app::bind().await;
    let counted = hits.clone();
    tokio::spawn(async move {
        let mut held = vec![];
        while let Ok((socket, _)) = listener.accept().await {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            held.push(socket);
        }
    });
    (base, hits)
}

const DEPLOY: &str = "/api/project/experiments/supplier-slip-solo-vs-team/deploy";
const RUNS: &str = "/api/project/experiments/supplier-slip-solo-vs-team/runs";

#[test]
fn deploy_starts_an_experiment_and_lists_it() {
    run(async {
        let dir = runnable_project();
        let fx = deploy_app("deploy-http", dir.path()).await;
        let (base, hits) = counting_provider().await;
        store_key(&fx, "anthropic-main", "openai", &base, "sk-model").await;
        let (status, detail) = fx.post(DEPLOY, json!({})).await;
        assert_eq!(status, 200, "{detail}");
        // The built-in agent runs on the stored key's server, not the default provider.
        wait_for(|| hits.load(std::sync::atomic::Ordering::SeqCst) > 0).await;
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

#[test]
fn a_key_that_is_not_stored_refuses_the_deploy() {
    run(async {
        let dir = runnable_project();
        let fx = deploy_app("deploy-nokey", dir.path()).await;
        let (status, refused) = fx.post(DEPLOY, json!({})).await;
        assert_eq!(status, 400, "{refused}");
        let text = refused.to_string();
        assert!(
            text.contains("anthropic-main") && text.contains("solo-planner"),
            "{text}"
        );
        let (_, runs) = fx.get(RUNS).await;
        assert!(runs["deployments"].as_array().unwrap().is_empty());
    });
}

const ENDPOINT_SECRET: &str = "sk-endpoint-s3cret";

/// Polls `done` for up to half a minute.
async fn wait_for(done: impl Fn() -> bool) {
    for _ in 0..60 {
        if done() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    panic!("it did not happen in time");
}

/// An agent endpoint that records the `Authorization` header of each request and answers done.
async fn endpoint(seen: Arc<Mutex<Vec<String>>>) -> String {
    let take = |State(seen): State<Arc<Mutex<Vec<String>>>>, headers: HeaderMap| async move {
        let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
        seen.lock()
            .unwrap()
            .push(auth.unwrap_or_default().to_string());
        Json(json!({"ok": true}))
    };
    let app = Router::new().route("/planner", post(take)).with_state(seen);
    let (listener, base) = app::bind().await;
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("{base}/planner")
}

/// The example project with only the endpoint setup, which calls `url` with `auth`.
fn endpoint_project(url: &str, auth: &str) -> tempfile::TempDir {
    let dir = runnable_project();
    let edit = |file: &str, from: &str, to: &str| {
        let path = dir.path().join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} lacks {from}");
        std::fs::write(path, text.replacen(from, to, 1)).unwrap();
    };
    let exp = "experiments/supplier-slip-solo-vs-team.yaml";
    edit(exp, "[solo-planner]", "[planner-with-rules]");
    let setup = "setups/planner-with-rules.yaml";
    edit(setup, "https://agents.larkspur.internal/planner", url);
    edit(setup, "auth: {key: planner-endpoint}", auth);
    dir
}

const KEY_AUTH: &str = "auth: {key: planner-endpoint}";

/// Everything a deployment left behind: its answers, and each branch's events and record.
async fn stored_text(fx: &AppFx, detail: &Value) -> Vec<String> {
    let (_, runs) = fx.get(RUNS).await;
    let mut stored = vec![detail.to_string(), runs.to_string()];
    for branch in branch_ids(&runs) {
        let evs = records(&fx.state.store).events(&branch).await.unwrap();
        stored.push(serde_json::to_string(&evs).unwrap());
        let b = records(&fx.state.store).branch(&branch).await.unwrap();
        stored.push(serde_json::to_string(&b).unwrap());
    }
    assert!(stored.len() > 2, "no branch found in {runs}");
    stored
}

#[test]
fn an_endpoints_auth_reaches_it_and_no_record_holds_the_secret() {
    run(async {
        let seen = Arc::new(Mutex::new(vec![]));
        let dir = endpoint_project(&endpoint(seen.clone()).await, KEY_AUTH);
        let fx = deploy_app("deploy-auth", dir.path()).await;
        let base = silent_provider().await;
        store_key(&fx, "planner-endpoint", "custom", &base, ENDPOINT_SECRET).await;
        let (status, detail) = fx.post(DEPLOY, json!({})).await;
        assert_eq!(status, 200, "{detail}");
        wait_for(|| !seen.lock().unwrap().is_empty()).await;
        let want = format!("Bearer {ENDPOINT_SECRET}");
        assert_eq!(seen.lock().unwrap().first(), Some(&want));
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        for text in stored_text(&fx, &detail).await {
            assert!(!text.contains(ENDPOINT_SECRET), "the secret leaked: {text}");
            assert!(
                !text.contains("{\"secret\""),
                "a reference to it leaked: {text}"
            );
        }
    });
}

#[test]
fn a_stored_team_still_authenticates_without_any_kept_value() {
    run(async {
        let seen = Arc::new(Mutex::new(vec![]));
        let dir = endpoint_project(&endpoint(seen.clone()).await, KEY_AUTH);
        let fx = deploy_app("deploy-again", dir.path()).await;
        let base = silent_provider().await;
        store_key(&fx, "planner-endpoint", "custom", &base, ENDPOINT_SECRET).await;
        let (_, detail) = fx.post(DEPLOY, json!({})).await;
        wait_for(|| !seen.lock().unwrap().is_empty()).await;
        // The team as stored holds a reference to the key's name and nothing kept in memory, so a
        // shaked that started since reads it the same way: start a run from the stored team.
        let branch = branch_ids(&fx.get(RUNS).await.1).remove(0);
        let team = records(&fx.state.store).branch(&branch).await.unwrap().team;
        let agent = serde_json::to_value(&team).unwrap()["agents"][0].clone();
        assert_eq!(
            agent["auth"],
            json!({"key": "planner-endpoint"}),
            "{detail}"
        );
        assert!(agent.get("headers").is_none(), "{agent}");
        let body =
            json!({"task_id": "t1", "team": team, "runner": "chain", "benchmark": "toybench"});
        let (status, started) = fx.post("/api/runs", body).await;
        assert_eq!(status, 200, "{started}");
        wait_for(|| seen.lock().unwrap().len() >= 2).await;
        assert_eq!(seen.lock().unwrap()[1], format!("Bearer {ENDPOINT_SECRET}"));
    });
}

/// The deploy's answer for `project`'s endpoint setup with a key of `kind` and `secret` stored.
async fn deploy_refused(
    tag: &str,
    auth: &str,
    key: Option<(&str, &str)>,
    layer: Option<Arc<dyn shaked::layer::Layer>>,
) -> String {
    let dir = endpoint_project("http://127.0.0.1:9/planner", auth);
    let fx = deploy_app_on(tag, dir.path(), layer).await;
    if let Some((kind, secret)) = key {
        store_key(
            &fx,
            "planner-endpoint",
            kind,
            &silent_provider().await,
            secret,
        )
        .await;
    }
    let (status, refused) = fx.post(DEPLOY, json!({})).await;
    assert_eq!(status, 400, "{refused}");
    let (_, runs) = fx.get(RUNS).await;
    assert!(runs["deployments"].as_array().unwrap().is_empty());
    refused.to_string()
}

#[test]
fn an_auth_that_may_not_be_sent_is_refused_at_deploy() {
    run(async {
        let key = Some(("custom", ENDPOINT_SECRET));
        let hosted = shaked::layer::Policy {
            environment_keys: false,
            ..shaked::layer::Policy::LOCAL
        };
        let hosted = Arc::new(shaked::harness::SpyLayer::new().with_policy(hosted));
        let env = "auth: {env: SHAKEDOWN_AGENT_TOKEN}";
        let text = deploy_refused("deploy-hosted-env", env, None, Some(hosted)).await;
        assert!(text.contains("not read here"), "{text}");
        let env = "auth: {env: SHAKEDOWN_VAULT_KEY}";
        let text = deploy_refused("deploy-bad-env", env, None, None).await;
        assert!(text.contains("SHAKEDOWN_AGENT_"), "{text}");
        let both = "auth: {key: planner-endpoint, env: SHAKEDOWN_AGENT_T}";
        let text = deploy_refused("deploy-both", both, key, None).await;
        assert!(text.contains("name one"), "{text}");
        let text = deploy_refused("deploy-model", KEY_AUTH, Some(("openai", "sk-m")), None).await;
        assert!(text.contains("model provider key"), "{text}");
        let text = deploy_refused("deploy-empty", KEY_AUTH, Some(("custom", "")), None).await;
        assert!(text.contains("empty"), "{text}");
        let shared = Arc::new(shaked::harness::SpyLayer::new().sharing_keys());
        let text = deploy_refused("deploy-shared", KEY_AUTH, key, Some(shared)).await;
        assert!(text.contains("shared with you"), "{text}");
        let text = deploy_refused("deploy-nokey-auth", KEY_AUTH, None, None).await;
        assert!(text.contains("planner-endpoint"), "{text}");
    });
}

/// A second caller citing the first's reference or key gets nothing of it.
#[test]
fn another_caller_cannot_use_a_reference_it_did_not_make() {
    run(async {
        let layer = Arc::new(shaked::harness::SpyLayer::new().scoping_by_token());
        let dir = runnable_project();
        let fx = deploy_app_on("deploy-scopes", dir.path(), Some(layer)).await;
        let (_, minted) = fx
            .post(
                "/api/tokens",
                json!({"name": "w", "scopes": ["read", "run", "admin"]}),
            )
            .await;
        let token = minted["token"].as_str().unwrap().to_string();
        let url = endpoint(Arc::new(Mutex::new(vec![]))).await;
        let agent = json!({"kind": "http", "url": url, "headers": {"X-Key": ENDPOINT_SECRET}});
        let (status, checked) = fx.post("/api/agents/check", agent).await;
        assert_eq!(status, 200, "{checked}");
        let reference = checked["agent"]["headers"].clone();
        assert!(reference["X-Key"]["secret"].is_string(), "{checked}");
        let cited = json!({"kind": "http", "url": url, "headers": reference});
        let again = fx
            .http
            .post(format!("{}/api/agents/check", fx.base))
            .bearer_auth(&token)
            .json(&cited)
            .send()
            .await
            .unwrap();
        let text = again.text().await.unwrap();
        assert!(text.contains("connect the agent again"), "{text}");
        assert!(!text.contains(ENDPOINT_SECRET));
    });
}
/// Every string under a `branch_id` or `branch` key in `v`.
fn branch_ids(v: &Value) -> Vec<String> {
    match v {
        Value::Object(m) => m
            .iter()
            .flat_map(|(k, x)| match (k.as_str(), x) {
                ("branch_id" | "branch", Value::String(s)) => vec![s.clone()],
                _ => branch_ids(x),
            })
            .collect(),
        Value::Array(a) => a.iter().flat_map(branch_ids).collect(),
        _ => vec![],
    }
}
