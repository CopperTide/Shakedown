//! Code connections: saved commands that start a team's driver, which shaked launches for each run
//! (`crate::driver`). The fixture drivers in `fixtures/drivers` run no model: they test how shaked
//! starts, watches and ends a driver. The live test runs a real one.

mod support;
use support::records;

use std::{future::Future, path::PathBuf, time::Duration};

use serde_json::{json, Value};
use shaked::model::*;
use support::{app::send, poll_until, AppFx, SAVES_BUILT, STEP, TIME_UP_LATE_MS, WAIT};

fn run<F: Future>(f: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(f)
}

async fn setup(tag: &str) -> AppFx {
    let tweak = |c: &mut shaked::app::Config| {
        c.driver_grace = Duration::from_secs(2);
        c.code_run_limit = Duration::from_secs(4);
    };
    AppFx::idle("connections", tag).config(tweak).start().await
}

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/drivers")
        .join(name);
    p.to_string_lossy().into_owned()
}

impl AppFx {
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

    /// Starts the experiment `exp` and waits up to `limit` for it to end: its runs' branches
    /// (those started by then).
    async fn experiment_runs(&self, exp: Value, limit: Duration) -> Vec<String> {
        let (status, made) = self.post("/api/experiments", exp).await;
        assert_eq!(status, 200, "{made}");
        let id = made["id"].as_str().unwrap().to_string();
        let store = &self.state.store;
        let mut branches = vec![];
        let done = async || {
            let suites = records(store).experiment_suites(&id).await.unwrap();
            let slots = records(store).slots(&suites[0].id).await.unwrap();
            branches = slots.iter().filter_map(|s| s.branch_id.clone()).collect();
            let ended = records(store).experiment_ended(&id).await.unwrap();
            (ended && branches.len() == slots.len()).then_some(())
        };
        poll_until(limit, STEP, done).await;
        branches
    }

    /// The branch once it has ended. A run of task 27 starts and is scored on the test's own hub,
    /// its saves built behind every other test's, so the wait is [`SAVES_BUILT`].
    async fn ended(&self, branch: &str) -> Branch {
        let over = async || {
            let b = records(&self.state.store).branch(branch).await.unwrap();
            (b.status != BranchStatus::Running).then_some(b)
        };
        let b = poll_until(SAVES_BUILT, STEP, over).await;
        b.unwrap_or_else(|| panic!("branch {branch} did not end"))
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
        let evs = records(&fx.state.store).events(&branch).await.unwrap();
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
    let gone = async || (!proc_dir.exists()).then_some(());
    poll_until(wait, STEP, gone).await.is_some()
}

#[test]
fn a_driver_that_outlives_its_run_is_killed() {
    run(async {
        let fx = setup("outlive").await;
        let dir =
            std::env::temp_dir().join(format!("shaked-outlive-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid_file = dir.join("pid");
        let id = fx
            .connection("outlive.py", json!({"PID_FILE": pid_file}))
            .await;
        let branch = fx.start(&id).await;
        let b = fx.ended(&branch).await;
        assert_eq!(b.status, BranchStatus::Failed, "{:?}", b.error);
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        let gone = gone_within(pid.trim(), WAIT).await;
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
        let branches = fx.experiment_runs(exp, SAVES_BUILT).await;
        assert_eq!(branches.len(), 2, "both runs started");
        for b in &branches {
            let ended = fx.ended(b).await;
            assert_eq!(ended.status, BranchStatus::Failed);
            assert!(ended.error.unwrap_or_default().contains("code 3"));
            let evs = records(&fx.state.store).events(b).await.unwrap();
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
async fn driver_exit(fx: &AppFx, branch: &str) -> Event {
    let exit = async || {
        let evs = records(&fx.state.store).events(branch).await.unwrap();
        evs.into_iter().rev().find(|e| {
            e.kind == EventKind::WorldEvent
                && e.payload["part"] == "driver"
                && e.branch_id == branch
        })
    };
    let exit = poll_until(WAIT, STEP, exit).await;
    exit.unwrap_or_else(|| panic!("no driver exit on {branch}"))
}

impl AppFx {
    /// Forks `branch` at `at`, expecting its driver relaunched: the new branch.
    async fn fork_relaunched(&self, branch: &str, at: i64) -> (String, i64) {
        let (status, forked) = self.fork(branch, json!({"at_seq": at})).await;
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
        let env = json!({"SHAKEDOWN_PROVIDER": "openai", "SHAKEDOWN_MODEL": "gpt-4.1-mini"});
        let body = json!({"name": "OpenAI Agents team", "command": command, "workdir": sdk_dir(), "env": env});
        let (status, conn) = self.post("/api/connections", body).await;
        assert_eq!(status, 200, "{conn}");
        conn["id"].as_str().unwrap().to_string()
    }
}

/// A fork of a Code run whose connection was deleted since: the fork is made, its driver
/// `failed`, and the new branch ends `failed` saying the connection is gone.
#[test]
fn a_fork_of_a_code_run_whose_connection_is_gone_fails() {
    run(async {
        let fx = setup("fork-gone").await;
        let id = fx.connection("fork_aware.py", json!({})).await;
        let parent = fx.start(&id).await;
        fx.ended(&parent).await;
        assert_eq!(fx.delete(&format!("/api/connections/{id}")).await.0, 200);
        let evs = records(&fx.state.store).events(&parent).await.unwrap();
        let at = evs.iter().find(|e| e.kind == EventKind::AgentStart);
        let body = json!({"at_seq": at.unwrap().seq});
        let (status, forked) = fx.fork(&parent, body).await;
        assert_eq!(
            (status, &forked["driver"]),
            (200, &json!("failed")),
            "{forked}"
        );
        let ended = fx.ended(forked["branch_id"].as_str().unwrap()).await;
        let why = format!("the Code connection {id} was deleted");
        assert_eq!(
            (ended.status, ended.error),
            (BranchStatus::Failed, Some(why))
        );
    })
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
        let evs = records(&fx.state.store).events(&parent).await.unwrap();
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
        let child_evs = records(&fx.state.store).events(child).await.unwrap();
        let scored = |e: &Event| e.kind == EventKind::Check && e.branch_id == child;
        assert!(child_evs.iter().any(scored));
    })
}

#[test]
fn testing_a_connection_says_what_its_agents_did() {
    run(async {
        let fx = setup("test-route").await;
        let id = fx.connection("fork_aware.py", json!({})).await;
        let (status, t) = fx
            .post(&format!("/api/connections/{id}/test"), json!({}))
            .await;
        assert_eq!(status, 200, "{t}");
        assert_eq!(t["agents"], json!(["worker"]), "{t}");
        assert_eq!(
            (t["tool_calls"].as_u64(), t["exit"].as_str()),
            (Some(0), Some("exit 0")),
            "{t}"
        );
        assert_eq!(
            (t["ok"].as_bool(), t["error"].as_str()),
            (Some(false), Some("no agent called a tool")),
            "{t}"
        );
        assert!(
            t["stderr_tail"].as_str().unwrap().contains("fork of:"),
            "{t}"
        );
        let runs = records(&fx.state.store).runs().await.unwrap();
        assert!(runs
            .iter()
            .any(|r| r.name.as_deref() == Some("Connection test · fork_aware.py")));
    })
}

#[test]
fn a_driver_that_never_finishes_is_cut_off_and_its_run_scored() {
    run(async {
        let fx = setup("hang").await;
        let id = fx.connection("hang.py", json!({})).await;
        let branch = fx.start(&id).await;
        let b = fx.ended(&branch).await;
        assert_eq!(b.status, BranchStatus::Failed);
        let error = b.error.unwrap_or_default();
        assert!(error.contains("still running after"), "{error}");
        let evs = records(&fx.state.store).events(&branch).await.unwrap();
        assert!(evs.iter().any(|e| e.kind == EventKind::Check));
        driver_exit(&fx, &branch).await;
    })
}

#[test]
fn a_code_setups_runs_cannot_be_claimed() {
    run(async {
        let fx = setup("claim").await;
        let id = fx.connection("exit_early.py", json!({})).await;
        let exp = json!({"task": "27", "runs": 1, "setups": [{"connection": id}]});
        let (status, made) = fx.post("/api/experiments", exp).await;
        assert_eq!(status, 200, "{made}");
        let suites = (fx.state.store)
            .at(crate::support::local())
            .experiment_suites(made["id"].as_str().unwrap())
            .await
            .unwrap();
        let path = format!("/api/suites/{}/claim", suites[0].id);
        assert_eq!(fx.post(&path, json!({})).await.0, 409);
    })
}

/// Each route that can start a driver, with a body it would take: the connection `id`, a
/// finished Code run `parent`.
fn launch_routes(id: &str, parent: &str) -> Vec<(String, Value)> {
    let team = json!({"id": "own", "name": "Own", "agents": []});
    let setups = json!([{"connection": id}]);
    vec![
        (
            "/api/connections".into(),
            json!({"name": "x", "command": ["python3"]}),
        ),
        (format!("/api/connections/{id}/test"), json!({})),
        (
            "/api/runs".into(),
            json!({"task_id": "27", "team": team, "connection": id}),
        ),
        (
            "/api/experiments".into(),
            json!({"task": "27", "runs": 1, "setups": setups}),
        ),
        (format!("/api/branches/{parent}/fork"), json!({"at_seq": 2})),
    ]
}

#[test]
fn every_route_that_starts_a_driver_refuses_a_web_page() {
    run(async {
        let fx = setup("guard").await;
        let id = fx.connection("fork_aware.py", json!({})).await;
        let parent = fx.start(&id).await;
        fx.ended(&parent).await;
        let routes = launch_routes(&id, &parent);
        for (path, body) in routes {
            let url = format!("{}{path}", fx.base);
            let foreign = fx
                .http
                .post(&url)
                .header("origin", "http://evil.test")
                .json(&body);
            assert_eq!(send(foreign).await.0, 403, "{path}");
            let plain =
                (fx.http.post(&url).header("content-type", "text/plain")).body(body.to_string());
            assert_eq!(send(plain).await.0, 415, "{path}");
        }
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
        let branches = fx.experiment_runs(exp, Duration::from_secs(600)).await;
        assert_eq!(branches.len(), 2, "both runs started");
        for b in &branches {
            let ended = fx.ended(b).await;
            let evs = records(&fx.state.store).events(b).await.unwrap();
            assert_eq!(evs[0].payload["connection"], json!(id), "{b}");
            let worked = evs.iter().any(|e| e.kind == EventKind::ToolCall);
            assert!(worked, "{b}: {:?}", ended.error);
            assert!(evs.iter().any(|e| e.kind == EventKind::Check), "{b}");
        }
    })
}

/// An app with the providers the environment configures (the live test's gateway).
async fn live_setup() -> AppFx {
    AppFx::configured("connections", "live").start().await
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
        let evs = records(&fx.state.store).events(&parent).await.unwrap();
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
        let mine = records(&fx.state.store).events(&child).await.unwrap();
        let has = |k: EventKind, from: i64| mine.iter().any(|e| e.kind == k && e.seq >= from);
        assert!(
            has(EventKind::ToolCall, start),
            "the team worked after the fork point"
        );
        assert!(has(EventKind::Check, start));
    })
}

impl AppFx {
    /// Starts a run of task 27 with the connection `id` and a world time window of `minutes`.
    async fn start_windowed(&self, id: &str, minutes: f64) -> String {
        let team = json!({"id": "own", "name": "Own", "agents": []});
        let body =
            json!({"task_id": "27", "team": team, "connection": id, "world_minutes": minutes});
        let (status, v) = self.post("/api/runs", body).await;
        assert_eq!(status, 200, "{v}");
        v["branch_id"].as_str().unwrap().to_string()
    }
}

/// The branch's events have its window's end.
fn has_time_up(evs: &[Event]) -> bool {
    let up = |e: &Event| e.kind == EventKind::WorldEvent && e.payload["part"] == "window";
    evs.iter().any(|e| up(e) && e.payload["kind"] == "time_up")
}

/// The window ended at clock `limit` or up to [`TIME_UP_LATE_MS`] later.
fn time_up_on_time(evs: &[Event], limit: i64) {
    let up = |e: &&Event| e.kind == EventKind::WorldEvent && e.payload["kind"] == "time_up";
    let at = evs
        .iter()
        .find(up)
        .map(|e| e.payload["clock_ms"].as_i64().unwrap());
    let on_time = at.is_some_and(|ms| (limit..=limit + TIME_UP_LATE_MS).contains(&ms));
    assert!(on_time, "time_up at {at:?}, not from {limit} ms");
}

#[test]
fn a_driver_that_exits_holding_a_finish_leaves_its_run_to_the_window() {
    run(async {
        let fx = setup("held-exit").await;
        let id = fx.connection("finish_early.py", json!({})).await;
        let branch = fx.start_windowed(&id, 0.1).await;
        let b = fx.ended(&branch).await;
        let error = b.error.unwrap_or_default();
        assert!(!error.contains("driver"), "{error}");
        let evs = records(&fx.state.store).events(&branch).await.unwrap();
        assert!(has_time_up(&evs), "the window ended the run");
        time_up_on_time(&evs, 6000);
        assert!(evs.iter().any(|e| e.kind == EventKind::Check));
        driver_exit(&fx, &branch).await;
    })
}

#[test]
fn a_windowed_run_is_not_cut_off_before_its_window_ends() {
    run(async {
        let fx = setup("window-cut").await; // drivers are cut off after 4 s
        let id = fx.connection("hang.py", json!({})).await;
        let branch = fx.start_windowed(&id, 0.12).await; // 7.2 s of world time
        let b = fx.ended(&branch).await;
        let error = b.error.unwrap_or_default();
        assert!(!error.contains("still running after"), "{error}");
        let evs = records(&fx.state.store).events(&branch).await.unwrap();
        assert!(has_time_up(&evs), "the window ended the run");
        time_up_on_time(&evs, 7200);
    })
}
