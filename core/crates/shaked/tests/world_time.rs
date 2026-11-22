//! World time: parts get the world's epoch and clock speed at `start` (a task's `clock_start` read
//! as UTC, else the run's start), forks keep their run's epoch, and the clock route tells a driver
//! the world's time. The run is one an external system drives; no model answers anything here.
mod support;
use support::records;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use shaked::model::*;
use support::{packages, run, AppFx};

/// An app over a toy clock part `box` that logs its `start`s and `advance`s, and a live external
/// run of task `t1` with the agent `planner`.
struct TimeFx {
    fx: AppFx,
    branch: String,
    log: std::path::PathBuf,
}

impl TimeFx {
    async fn new(tag: &str, bench_args: &[&str]) -> TimeFx {
        let log =
            std::env::temp_dir().join(format!("shaked-time-{tag}-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&log);
        let log_arg = log.to_string_lossy().into_owned();
        let part = ["--uses-clock", "--log-time", log_arg.as_str()];
        let root = packages::toy_world(tag, &part, bench_args);
        let fx = AppFx::idle("worldtime", tag)
            .root(root)
            .bench("toybench")
            .start()
            .await;
        let team = json!({"id": "own", "name": "Own design", "agents": []});
        let body = json!({"task_id": "t1", "team": team, "runner": "external"});
        let (status, v) = fx.post("/api/runs", body).await;
        assert_eq!(status, 200, "{v}");
        let branch = v["branch_id"].as_str().unwrap().to_string();
        let path = format!("/api/branches/{branch}/agents");
        let (status, a) = fx
            .post(&path, json!({"id": "planner", "name": "Planner"}))
            .await;
        assert_eq!(status, 200, "{a}");
        TimeFx { fx, branch, log }
    }

    fn lines(&self) -> Vec<Vec<String>> {
        let text = std::fs::read_to_string(&self.log).unwrap_or_default();
        text.lines()
            .map(|l| l.split(' ').map(str::to_string).collect())
            .collect()
    }

    /// The `start` lines that carried a world time: `(instance, epoch_ms)`.
    fn timed_starts(&self) -> Vec<(String, i64)> {
        let starts = self
            .lines()
            .into_iter()
            .filter(|l| l[0] == "start" && l[2] != "none");
        starts
            .map(|l| (l[1].clone(), l[2].parse().unwrap()))
            .collect()
    }

    async fn on(&self, branch: &str, what: &str) -> (u16, Value) {
        self.fx
            .post(&format!("/api/branches/{branch}/{what}"), json!({}))
            .await
    }
}

impl Drop for TimeFx {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.log);
    }
}

fn wall_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// `(clock_ms, wall_ms)` of the last advance before `before` and the first after `after`.
fn advances_around(lines: &[Vec<String>], before: i64, after: i64) -> ((i64, i64), (i64, i64)) {
    let advances: Vec<(i64, i64)> = (lines.iter().filter(|l| l[0] == "advance"))
        .map(|l| (l[2].parse().unwrap(), l[3].parse().unwrap()))
        .collect();
    let last = advances
        .iter()
        .rev()
        .find(|a| a.1 < before)
        .copied()
        .unwrap();
    let first = advances.iter().find(|a| a.1 > after).copied().unwrap();
    (last, first)
}

#[test]
fn a_clock_parts_stamps_leave_out_the_time_the_branch_was_paused() {
    run(async {
        let fx = TimeFx::new("paused", &["--clock-start", "2026-03-10T06:00:00"]).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(fx.timed_starts()[0].1, 1_773_122_400_000);
        let paused_at = wall_ms();
        assert_eq!(fx.on(&fx.branch, "pause").await.0, 200);
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(fx.on(&fx.branch, "resume").await.0, 200);
        let resumed_at = wall_ms();
        tokio::time::sleep(Duration::from_millis(500)).await;
        let (before, after) = advances_around(&fx.lines(), paused_at, resumed_at);
        let (world, wall) = (after.0 - before.0, after.1 - before.1);
        // ended first, so the clock part writes no more to the log the fixture removes
        assert_eq!(fx.on(&fx.branch, "finish").await.0, 200);
        assert!(wall - world >= 400, "wall {wall} ms, world {world} ms");
    })
}

#[test]
fn a_fork_keeps_its_runs_epoch() {
    run(async {
        let fx = TimeFx::new("fork", &[]).await;
        let evs = records(&fx.fx.state.store)
            .events(&fx.branch)
            .await
            .unwrap();
        let epoch = evs[0].payload["world_epoch_ms"].as_i64().unwrap();
        let start = evs
            .iter()
            .find(|e| e.kind == EventKind::AgentStart)
            .unwrap()
            .seq;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(fx.on(&fx.branch, "finish").await.0, 200);
        let body = json!({"at_seq": start, "changes": {}});
        let (status, v) = fx
            .fx
            .post(&format!("/api/branches/{}/fork", fx.branch), body)
            .await;
        assert_eq!(status, 200, "{v}");
        let starts = fx.timed_starts();
        assert_eq!(starts.len(), 2, "{starts:?}");
        assert_eq!((starts[0].1, starts[1].1), (epoch, epoch));
        assert_ne!(starts[0].0, starts[1].0);
        assert_eq!(
            fx.on(v["branch_id"].as_str().unwrap(), "finish").await.0,
            200
        );
    })
}

#[test]
fn the_clock_route_tells_world_time() {
    run(async {
        let fx = TimeFx::new("route", &["--clock-start", "2026-03-10T06:00:00"]).await;
        let path = format!("/api/branches/{}/clock", fx.branch);
        let (status, v) = fx.fx.get(&path).await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(v["world_epoch_ms"], 1_773_122_400_000i64);
        let (clock, world) = (
            v["clock_ms"].as_i64().unwrap(),
            v["world_ms"].as_i64().unwrap(),
        );
        assert!((world - 1_773_122_400_000 - clock).abs() <= 50, "{v}");
        assert!(v["speed"].as_f64().unwrap() > 0.0 && v["paused"] == false);
        assert_eq!(fx.on(&fx.branch, "pause").await.0, 200);
        assert_eq!(fx.fx.get(&path).await.1["paused"], true);
        assert_eq!(fx.on(&fx.branch, "finish").await.0, 200);
        assert_eq!(fx.fx.get(&path).await.0, 409);
    })
}

/// A branch of the toy world `toybench` (a clock part `box` started with `part_args`) on a
/// stepped clock whose writes cost two world minutes, made by hand: no runner moves it.
struct SteppedFx {
    ctx: std::sync::Arc<shaked::exec::BranchCtx>,
    exec: shaked::exec::ToolExec,
    store: std::sync::Arc<shaked::store::Store>,
    _clean: support::pg::Cleanup,
    _saves: std::path::PathBuf,
    _root: packages::ToyRoot,
}

impl SteppedFx {
    async fn new(tag: &str, part_args: &[&str]) -> SteppedFx {
        let root = packages::toy_world(tag, part_args, &[]);
        let saves =
            std::env::temp_dir().join(format!("shaked-stepped-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&saves).unwrap();
        let registry = parts::Registry::scan(std::slice::from_ref(&root.0)).unwrap();
        let hub = std::sync::Arc::new(parts::Hub::new(
            registry,
            packages::bin_dir(),
            saves.clone(),
        ));
        let (store, clean) = support::app::meta_store("stepped", tag).await;
        let team = Team {
            id: "t".into(),
            name: "T".into(),
            agents: vec![AgentSpec::new("planner", AgentKind::Open)],
        };
        let run_id = records(&store).create_run("t1", &team).await.unwrap();
        let branch = records(&store)
            .create_branch(&run_id, None, &team, &json!([]), None)
            .await
            .unwrap();
        let bench = hub.bench_describe("toybench").await.unwrap();
        let from = shaked::world::WorldFrom::Base;
        let world = shaked::world::World::start(hub.clone(), &bench, &branch, 1, from).await;
        let cost = shaked::clock::StepCost {
            read_ms: 0,
            write_ms: 120_000,
            tools: Default::default(),
        };
        let clock = shaked::clock::BranchClock::stepped(0, cost);
        let ctx = shaked::exec::BranchCtx::new(
            branch,
            std::sync::Arc::new(world.unwrap()),
            team,
            "t1".into(),
        )
        .with_clock(clock);
        SteppedFx {
            ctx: std::sync::Arc::new(ctx),
            exec: shaked::exec::ToolExec::new(store.clone()),
            store,
            _clean: clean,
            _saves: saves,
            _root: root,
        }
    }

    async fn call(&self, tool: &str) -> Value {
        self.exec
            .run(&self.ctx, "planner", tool, json!({}), None)
            .await
    }

    /// The `clock_ms` each recorded tool call ran at.
    async fn call_clocks(&self) -> Vec<i64> {
        let evs = records(&self.store)
            .events(&self.ctx.branch_id)
            .await
            .unwrap();
        evs.iter()
            .filter(|e| e.kind == EventKind::ToolCall)
            .map(|e| e.payload["clock_ms"].as_i64().unwrap())
            .collect()
    }
}

impl Drop for SteppedFx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self._saves);
    }
}

/// On a stepped clock a write moves world time by its cost after the call (the call runs at the
/// clock before it), a read costs nothing here, and no ticker moves the clock meanwhile.
#[test]
fn tool_call_charges_world_minutes() {
    run(async {
        let fx = SteppedFx::new("charge", &["--uses-clock"]).await;
        fx.ctx.start_ticker(&fx.store).await;
        assert!(
            !shaked::ticker::is_running(&fx.ctx.branch_id),
            "a stepped branch has no ticker"
        );
        assert!(fx.call("poke").await.get("error").is_none());
        assert_eq!(fx.ctx.clock.now_ms(), 120_000);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(fx.call("peek").await.get("error").is_none());
        assert_eq!(
            fx.ctx.clock.now_ms(),
            120_000,
            "a read costs nothing, and wall time none"
        );
        fx.call("poke").await;
        assert_eq!(fx.call_clocks().await, [0, 120_000, 120_000]);
        assert_eq!(fx.ctx.clock.now_ms(), 240_000);
        fx.ctx.world.stop().await;
    })
}

/// A chat answer with `content` and no tool call: the agent's turn ends.
fn says(content: &str) -> Value {
    json!({"id": "c", "object": "chat.completion", "model": "m",
        "choices": [{"index": 0, "finish_reason": "stop",
                     "message": {"role": "assistant", "content": content}}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})
}

/// A chat answer that calls `finish`.
fn finishes() -> Value {
    json!({"id": "c", "object": "chat.completion", "model": "m",
        "choices": [{"index": 0, "finish_reason": "tool_calls",
                     "message": {"role": "assistant", "content": null, "tool_calls": [
                        {"id": "call-1", "type": "function",
                         "function": {"name": "finish", "arguments": "{\"summary\":\"done\"}"}}]}}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})
}

const HOUR: i64 = 3_600_000;

/// A dispatch run of the toy world on a stepped clock (`clock`), its lead an API-mode agent on a
/// scripted server answering `answers` in order: the ended branch's events.
async fn stepped_run(
    tag: &str,
    part_args: &[&str],
    clock: Value,
    answers: Vec<Value>,
) -> (AppFx, Vec<Event>) {
    use shaked::providers::{Endpoint, LocalEndpoint, ProviderConfig};
    let llm = support::fake_llm::fake_llm(answers.into_iter().map(|a| (200, a)).collect()).await;
    let mut providers = ProviderConfig::default();
    providers.locals.push(LocalEndpoint {
        name: "pod".into(),
        endpoint: Endpoint::new(&llm.base, None),
        rate: None,
    });
    providers.default = "local:pod".into();
    let root = packages::toy_world(tag, part_args, &["--clock-start", "2026-03-10T06:00:00"]);
    let fx = AppFx::idle("stepped", tag)
        .root(root)
        .bench("toybench")
        .providers(providers)
        .start()
        .await;
    let mut lead = AgentSpec::new("lead", AgentKind::Open)
        .with_model("m")
        .with_provider("local:pod")
        .with_prompt("You lead.");
    lead.lead = true;
    let team = json!({"id": "t", "name": "T", "agents": [lead]});
    let scenario = json!({"disruptions": [], "clock": clock});
    let body = json!({"task_id": "t1", "team": team, "runner": "dispatch", "scenario": scenario});
    let (status, v) = fx.post("/api/runs", body).await;
    assert_eq!(status, 200, "{v}");
    let branch = v["branch_id"].as_str().unwrap().to_string();
    for _ in 0..600 {
        let (_, b) = fx.get(&format!("/api/branches/{branch}")).await;
        if !b["ended_at"].is_null() {
            let evs = records(&fx.state.store).events(&branch).await.unwrap();
            return (fx, evs);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the stepped run did not end");
}

/// The clock moves a quiet stepped world made: `(reason, clock_ms)`.
fn moves(evs: &[Event]) -> Vec<(String, i64)> {
    let mine = evs.iter().filter(|e| {
        e.kind == EventKind::WorldEvent
            && e.payload["part"] == "clock"
            && e.payload["kind"] == "advance"
    });
    mine.map(|e| {
        let reason = e.payload["data"]["reason"].as_str().unwrap().to_string();
        (reason, e.payload["clock_ms"].as_i64().unwrap())
    })
    .collect()
}

/// The lead's wakes: their reasons.
fn wake_reasons(evs: &[Event]) -> Vec<String> {
    let wakes = evs.iter().filter(|e| e.kind == EventKind::Wake);
    wakes
        .map(|e| e.payload["reason"].as_str().unwrap().to_string())
        .collect()
}

/// With no alert and no epoch, a quiet team's world goes straight to the horizon: the lead is
/// never woken again, and the idle rule never runs.
#[test]
fn quiet_world_runs_to_horizon_without_wakes() {
    run(async {
        let clock = json!({"write_ms": 120_000, "horizon_ms": 3 * HOUR});
        let (_fx, evs) = stepped_run(
            "quiet",
            &["--uses-clock"],
            clock,
            vec![says("Nothing to do.")],
        )
        .await;
        assert_eq!(wake_reasons(&evs), ["start"]);
        assert_eq!(moves(&evs), [("horizon".to_string(), 3 * HOUR)]);
        assert!(
            evs.iter().any(|e| e.kind == EventKind::Check),
            "scored at the horizon"
        );
        let first = evs.iter().find(|e| e.kind == EventKind::Wake).unwrap();
        assert!(
            first.payload["input"]
                .as_str()
                .unwrap()
                .contains("moves when the team acts or waits"),
            "{}",
            first.payload["input"]
        );
    })
}

/// A part's alert wakes the lead with the alert as mail at the world time it fired.
#[test]
fn alert_wakes_lead_with_mail() {
    run(async {
        let clock = json!({"write_ms": 120_000, "horizon_ms": 3 * HOUR});
        let part = ["--uses-clock", "--alert-at", "3600000"];
        let (_fx, evs) = stepped_run("alert", &part, clock, vec![says("Waiting.")]).await;
        assert_eq!(wake_reasons(&evs), ["start", "mail"]);
        let alert = evs
            .iter()
            .find(|e| e.kind == EventKind::WorldEvent && e.payload["kind"] == "alert")
            .unwrap();
        assert_eq!(alert.payload["clock_ms"], HOUR);
        let mail_wake = evs
            .iter()
            .filter(|e| e.kind == EventKind::Wake)
            .nth(1)
            .unwrap();
        assert_eq!(mail_wake.payload["mail"], json!([alert.seq]));
        assert!(mail_wake.payload["input"]
            .as_str()
            .unwrap()
            .contains("the toy needs a look"));
        assert_eq!(
            moves(&evs),
            [
                ("wake".to_string(), HOUR),
                ("horizon".to_string(), 3 * HOUR)
            ]
        );
    })
}

/// A lead that finishes before the world's events still has them land: the world runs on, with
/// no agent, through each wake to the horizon, recorded, and is scored there.
#[test]
fn finish_early_runs_to_horizon() {
    run(async {
        let clock = json!({"write_ms": 120_000, "horizon_ms": 3 * HOUR});
        let part = ["--uses-clock", "--alert-at", "3600000"];
        let (_fx, evs) = stepped_run("finish", &part, clock, vec![finishes(), says("Done.")]).await;
        let finish = evs
            .iter()
            .position(|e| e.kind == EventKind::Finish)
            .expect("finished");
        assert_eq!(
            moves(&evs[finish..]),
            [
                ("wake".to_string(), HOUR),
                ("horizon".to_string(), 3 * HOUR)
            ]
        );
        assert!(evs[finish..]
            .iter()
            .any(|e| e.kind == EventKind::WorldEvent && e.payload["kind"] == "alert"));
        assert_eq!(
            wake_reasons(&evs),
            ["start"],
            "no one is woken after the finish"
        );
        let check = evs
            .iter()
            .position(|e| e.kind == EventKind::Check)
            .expect("scored");
        assert!(check > finish);
    })
}

/// With no alert, the epoch gives the lead a look every hour of world time, and looks are no
/// idle wakes: the run reaches its horizon.
#[test]
fn epoch_gives_lead_a_look() {
    run(async {
        let clock = json!({"write_ms": 120_000, "epoch_ms": HOUR, "horizon_ms": 3 * HOUR});
        let (_fx, evs) =
            stepped_run("look", &["--uses-clock"], clock, vec![says("All calm.")]).await;
        assert_eq!(wake_reasons(&evs), ["start", "look", "look"]);
        assert_eq!(
            moves(&evs),
            [
                ("look".to_string(), HOUR),
                ("look".to_string(), 2 * HOUR),
                ("horizon".to_string(), 3 * HOUR)
            ]
        );
    })
}
