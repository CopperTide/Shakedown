//! Code connections: saved commands that start a team's driver, which arenad launches for each run
//! (`crate::driver`). The fixture drivers in `fixtures/drivers` run no model: they test how arenad
//! starts, watches and ends a driver. The live test runs a real one.

#[path = "support/app.rs"]
mod app_support;
#[path = "support/packages.rs"]
mod packages;
#[path = "support/pg.rs"]
mod pg;

use std::{future::Future, path::PathBuf, time::Duration};

use app_support::send;
use arenad::{app::AppState, model::*};
use packages::TestHub;
use pg::Cleanup;
use serde_json::{json, Value};
use std::sync::Arc;

fn run<F: Future>(f: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(f)
}

struct Fx {
    state: Arc<AppState>,
    _hub: TestHub,
    _clean: Cleanup,
    base: String,
    http: reqwest::Client,
}

async fn setup(tag: &str) -> Fx {
    let hub = TestHub::new(&[]);
    let (store, clean) = app_support::meta_store("connections", tag).await;
    let tweak = |c: &mut arenad::app::Config| c.driver_grace = Duration::from_secs(2);
    let (state, base) = app_support::idle_app(store, hub.hub.clone(), None, tweak).await;
    Fx {
        state,
        _hub: hub,
        _clean: clean,
        base,
        http: reqwest::Client::new(),
    }
}

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/drivers")
        .join(name);
    p.to_string_lossy().into_owned()
}

impl Fx {
    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        send(self.http.post(format!("{}{path}", self.base)).json(&body)).await
    }

    async fn get(&self, path: &str) -> (u16, Value) {
        send(self.http.get(format!("{}{path}", self.base))).await
    }

    /// Saves a connection running `python3 <fixture>` with `env`: its id.
    async fn connection(&self, script: &str, env: Value) -> String {
        let body = json!({"name": script, "command": ["python3", fixture(script)], "env": env});
        let (status, v) = self.post("/api/connections", body).await;
        assert_eq!(status, 200, "{v}");
        v["id"].as_str().unwrap().to_string()
    }

    /// Starts a run of task 27 with the connection `id`: its branch.
    async fn start(&self, id: &str) -> String {
        let body = json!({"task_id": "27", "team": {"id": "own", "name": "Own", "agents": []}, "connection": id});
        let (status, v) = self.post("/api/runs", body).await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(v["driver"], "launched", "{v}");
        v["branch_id"].as_str().unwrap().to_string()
    }

    /// Starts the experiment `exp` and waits up to `secs` for it to end: its runs' branches.
    async fn experiment_runs(&self, exp: Value, secs: u64) -> Vec<String> {
        let (status, made) = self.post("/api/experiments", exp).await;
        assert_eq!(status, 200, "{made}");
        let id = made["id"].as_str().unwrap().to_string();
        let store = &self.state.store;
        let deadline = std::time::Instant::now() + Duration::from_secs(secs);
        loop {
            let suites = store.experiment_suites(&id).await.unwrap();
            let slots = store.slots(&suites[0].id).await.unwrap();
            let branches: Vec<String> = slots.iter().filter_map(|s| s.branch_id.clone()).collect();
            let ended = store.experiment_ended(&id).await.unwrap();
            if ended && branches.len() == slots.len() || std::time::Instant::now() > deadline {
                return branches;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn ended(&self, branch: &str) -> Branch {
        for _ in 0..300 {
            let b = self.state.store.branch(branch).await.unwrap();
            if b.status != BranchStatus::Running {
                return b;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("branch {branch} did not end");
    }
}

#[test]
fn connections_are_saved_listed_and_redacted() {
    run(async {
        let fx = setup("crud").await;
        let env = json!({"OPENAI_API_KEY": "sk-secret", "MODE": "fast", "db_password": "pw"});
        let body =
            json!({"name": "LangGraph team", "command": ["python3", "driver.py"], "env": env});
        let (status, made) = fx.post("/api/connections", body).await;
        assert_eq!(status, 200, "{made}");
        assert_eq!(made["kind"], "code");
        assert_eq!(
            made["env"],
            json!({"OPENAI_API_KEY": "••••", "MODE": "fast", "db_password": "••••"})
        );
        let (_, list) = fx.get("/api/connections").await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["name"], "LangGraph team");
        assert_eq!(list[0]["command"], json!(["python3", "driver.py"]));
        assert_eq!(list[0]["env"]["OPENAI_API_KEY"], "••••");
        let id = made["id"].as_str().unwrap();
        let del = fx.http.delete(format!("{}/api/connections/{id}", fx.base));
        assert_eq!(send(del).await.0, 200);
        assert_eq!(fx.get("/api/connections").await.1, json!([]));
    })
}

#[test]
fn a_bad_connection_is_refused() {
    run(async {
        let fx = setup("bad").await;
        let empty = json!({"name": "x", "command": []});
        assert_eq!(fx.post("/api/connections", empty).await.0, 400);
        let nowhere = json!({"name": "x", "command": ["python3"], "workdir": "/no/such/folder"});
        assert_eq!(fx.post("/api/connections", nowhere).await.0, 400);
        let body = json!({"task_id": "27", "team": {"id": "own", "name": "Own", "agents": []}, "connection": "nope"});
        assert_eq!(fx.post("/api/runs", body).await.0, 404);
        assert_eq!(fx.get("/api/connections").await.1, json!([]));
    })
}

#[test]
fn a_web_page_cannot_save_or_start_a_connection() {
    run(async {
        let fx = setup("csrf").await;
        // A web page can't save one: a cross-site request, or one that isn't JSON, is refused.
        let ok = json!({"name": "x", "command": ["python3"]});
        let url = format!("{}/api/connections", fx.base);
        let foreign = fx
            .http
            .post(&url)
            .header("origin", "http://evil.test")
            .json(&ok);
        assert_eq!(send(foreign).await.0, 403);
        let plain = fx
            .http
            .post(&url)
            .header("content-type", "text/plain")
            .body(ok.to_string());
        assert_eq!(send(plain).await.0, 415);
        assert_eq!(fx.get("/api/connections").await.1, json!([]));
    })
}

#[test]
fn a_driver_that_exits_without_finishing_fails_the_run_scored() {
    run(async {
        let fx = setup("exit").await;
        let id = fx.connection("exit_early.py", json!({})).await;
        let branch = fx.start(&id).await;
        let b = fx.ended(&branch).await;
        assert_eq!(b.status, BranchStatus::Failed);
        let error = b.error.unwrap_or_default();
        assert!(
            error.contains("code 3") && error.contains("the team's code broke"),
            "{error}"
        );
        let evs = fx.state.store.events(&branch).await.unwrap();
        assert!(evs.iter().any(|e| e.kind == EventKind::Check));
        let exit = evs
            .iter()
            .find(|e| e.kind == EventKind::WorldEvent && e.payload["part"] == "driver");
        let exit = exit.expect("the driver's exit is recorded");
        assert_eq!(
            (exit.payload["kind"].as_str(), exit.payload["text"].as_str()),
            (Some("exit"), Some("exit 3"))
        );
        assert_eq!(evs[0].payload["connection"], json!(id));
    })
}

/// True once the process `pid` is gone, waiting up to `wait`.
async fn gone_within(pid: &str, wait: Duration) -> bool {
    let proc_dir = PathBuf::from(format!("/proc/{pid}"));
    let deadline = std::time::Instant::now() + wait;
    while proc_dir.exists() {
        if std::time::Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    true
}

#[test]
fn a_driver_that_outlives_its_run_is_killed() {
    run(async {
        let fx = setup("outlive").await;
        let dir =
            std::env::temp_dir().join(format!("arenad-outlive-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid_file = dir.join("pid");
        let id = fx
            .connection("outlive.py", json!({"PID_FILE": pid_file}))
            .await;
        let branch = fx.start(&id).await;
        let b = fx.ended(&branch).await;
        assert_eq!(b.status, BranchStatus::Failed, "{:?}", b.error);
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        let gone = gone_within(pid.trim(), Duration::from_secs(10)).await;
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            gone,
            "the driver still runs after its run ended and the grace passed"
        );
    })
}

#[test]
fn an_experiment_setup_launches_its_driver_for_each_run() {
    run(async {
        let fx = setup("experiment").await;
        let id = fx.connection("exit_early.py", json!({})).await;
        let exp =
            json!({"task": "27", "runs": 2, "setups": [{"name": "Our code", "connection": id}]});
        let branches = fx.experiment_runs(exp, 30).await;
        assert_eq!(branches.len(), 2, "both runs started");
        for b in &branches {
            let ended = fx.ended(b).await;
            assert_eq!(ended.status, BranchStatus::Failed);
            assert!(ended.error.unwrap_or_default().contains("code 3"));
            let evs = fx.state.store.events(b).await.unwrap();
            assert_eq!(evs[0].payload["connection"], json!(id));
        }
        // A setup's connection must exist, and a web page can't start one.
        let gone = json!({"task": "27", "runs": 1, "setups": [{"connection": "nope"}]});
        assert_eq!(fx.post("/api/experiments", gone).await.0, 404);
        let url = format!("{}/api/experiments", fx.base);
        let body = json!({"task": "27", "runs": 1, "setups": [{"connection": id}]});
        let foreign = fx
            .http
            .post(url)
            .header("origin", "http://evil.test")
            .json(&body);
        assert_eq!(send(foreign).await.0, 403);
    })
}

/// The `driver` exit event of `branch`, waiting for it (it is recorded just after the run ends).
async fn driver_exit(fx: &Fx, branch: &str) -> Event {
    for _ in 0..100 {
        let evs = fx.state.store.events(branch).await.unwrap();
        let exit = evs.into_iter().rev().find(|e| {
            e.kind == EventKind::WorldEvent
                && e.payload["part"] == "driver"
                && e.branch_id == branch
        });
        if let Some(e) = exit {
            return e;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("no driver exit on {branch}");
}

impl Fx {
    /// Forks `branch` at `at`, expecting its driver relaunched: the new branch.
    async fn fork_relaunched(&self, branch: &str, at: i64) -> (String, i64) {
        let path = format!("/api/branches/{branch}/fork");
        let (status, forked) = self.post(&path, json!({"at_seq": at})).await;
        assert_eq!(
            (status, forked["driver"].as_str()),
            (200, Some("relaunched")),
            "{forked}"
        );
        let child = forked["branch_id"].as_str().unwrap().to_string();
        (child, forked["start_seq"].as_i64().unwrap())
    }

    /// Saves a connection that runs the SDK's OpenAI Agents example driver on gpt-4.1-mini.
    async fn example_connection(&self) -> String {
        let driver = sdk_dir().join("examples/drivers/openai_agents_driver.py");
        let command = json!(["uv", "run", "--extra", "openai-agents", "python", driver]);
        let env = json!({"ARENA_PROVIDER": "openai", "ARENA_MODEL": "gpt-4.1-mini"});
        let body = json!({"name": "OpenAI Agents team", "command": command, "workdir": sdk_dir(), "env": env});
        let (status, conn) = self.post("/api/connections", body).await;
        assert_eq!(status, 200, "{conn}");
        conn["id"].as_str().unwrap().to_string()
    }
}

#[test]
fn a_fork_of_a_code_run_relaunches_the_driver() {
    run(async {
        let fx = setup("fork").await;
        let id = fx.connection("fork_aware.py", json!({})).await;
        let parent = fx.start(&id).await;
        fx.ended(&parent).await;
        let first = driver_exit(&fx, &parent).await;
        assert!(first.payload["data"]["stderr_tail"]
            .as_str()
            .unwrap()
            .contains("fork of: \n"));
        let evs = fx.state.store.events(&parent).await.unwrap();
        let at = evs
            .iter()
            .find(|e| e.kind == EventKind::AgentStart)
            .unwrap()
            .seq;
        let (child, _) = fx.fork_relaunched(&parent, at).await;
        let child = child.as_str();
        let ended = fx.ended(child).await;
        assert_ne!(ended.status, BranchStatus::Running);
        let again = driver_exit(&fx, child).await;
        let said = again.payload["data"]["stderr_tail"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(said.contains(&format!("fork of: {parent}")), "{said}");
        let child_evs = fx.state.store.events(child).await.unwrap();
        let scored = |e: &Event| e.kind == EventKind::Check && e.branch_id == child;
        assert!(child_evs.iter().any(scored));
    })
}

// ---------------------------------------------------------------- live: a real driver and model

/// The SDK's folder, where the example drivers and their environment are.
fn sdk_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../sdk/python")
}

/// Why the live test can't run here, if it can't: no key for the gateway, or no `uv` with the
/// openai-agents extra.
fn live_missing() -> Option<String> {
    if std::env::var("OPENAI_API_KEY").map_or(true, |k| k.trim().is_empty()) {
        return Some("OPENAI_API_KEY is not set (the gateway needs it)".into());
    }
    let probe = std::process::Command::new("uv")
        .args([
            "run",
            "--extra",
            "openai-agents",
            "python",
            "-c",
            "import agents",
        ])
        .current_dir(sdk_dir())
        .output();
    match probe {
        Ok(o) if o.status.success() => None,
        _ => Some("`uv run --extra openai-agents` can't import the OpenAI Agents SDK".into()),
    }
}

#[test]
#[ignore = "live: runs a real model through the gateway"]
fn live_an_experiment_setup_runs_a_code_connection() {
    if let Some(why) = live_missing() {
        eprintln!("skipped: {why}");
        return;
    }
    run(async {
        let fx = live_setup().await;
        let id = fx.example_connection().await;
        let id = id.as_str();
        let task = fx.get("/api/tasks").await.1[0]["id"].clone();
        let exp =
            json!({"task": task, "runs": 2, "setups": [{"connection": id}], "max_cost_usd": 1.0});
        let branches = fx.experiment_runs(exp, 600).await;
        assert_eq!(branches.len(), 2, "both runs started");
        for b in &branches {
            let ended = fx.ended(b).await;
            let evs = fx.state.store.events(b).await.unwrap();
            assert_eq!(evs[0].payload["connection"], json!(id), "{b}");
            let worked = evs.iter().any(|e| e.kind == EventKind::ToolCall);
            assert!(worked, "{b}: {:?}", ended.error);
            assert!(evs.iter().any(|e| e.kind == EventKind::Check), "{b}");
        }
    })
}

/// An app with the providers the environment configures (the live test's gateway).
async fn live_setup() -> Fx {
    let hub = TestHub::new(&[]);
    let (store, clean) = app_support::meta_store("connections", "live").await;
    let (state, base) = app_support::app(store, hub.hub.clone(), |_| {}).await;
    Fx {
        state,
        _hub: hub,
        _clean: clean,
        base,
        http: reqwest::Client::new(),
    }
}

#[test]
#[ignore = "live: runs a real model through the gateway"]
fn live_a_fork_of_a_code_run_relaunches_the_driver() {
    if let Some(why) = live_missing() {
        eprintln!("skipped: {why}");
        return;
    }
    run(async {
        let fx = live_setup().await;
        let parent = fx.start(&fx.example_connection().await).await;
        fx.ended(&parent).await;
        let evs = fx.state.store.events(&parent).await.unwrap();
        let calls: Vec<i64> = (evs.iter().filter(|e| e.kind == EventKind::ToolCall))
            .map(|e| e.seq)
            .collect();
        let at = *calls
            .get(1)
            .or(calls.first())
            .expect("the team called a tool");
        let (child, start) = fx.fork_relaunched(&parent, at).await;
        fx.ended(&child).await;
        driver_exit(&fx, &child).await;
        let mine = fx.state.store.events(&child).await.unwrap();
        let has = |k: EventKind, from: i64| mine.iter().any(|e| e.kind == k && e.seq >= from);
        assert!(
            has(EventKind::ToolCall, start),
            "the team worked after the fork point"
        );
        assert!(has(EventKind::Check, start));
    })
}
