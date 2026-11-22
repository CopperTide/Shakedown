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
        TimeFx::with_part(tag, &[], bench_args).await
    }

    /// `new` with the clock part also started with `part_args`.
    async fn with_part(tag: &str, part_args: &[&str], bench_args: &[&str]) -> TimeFx {
        TimeFx::build(tag, part_args, bench_args, None).await
    }

    /// `with_part`, shaked serving the project folder `project` when given.
    async fn build(
        tag: &str,
        part_args: &[&str],
        bench_args: &[&str],
        project: Option<std::path::PathBuf>,
    ) -> TimeFx {
        let log =
            std::env::temp_dir().join(format!("shaked-time-{tag}-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&log);
        let log_arg = log.to_string_lossy().into_owned();
        let mut part = vec!["--uses-clock", "--log-time", log_arg.as_str()];
        part.extend_from_slice(part_args);
        let root = packages::toy_world(tag, &part, bench_args);
        let fx = AppFx::idle("worldtime", tag)
            .root(root)
            .bench("toybench")
            .config(move |c| c.project_dir = project)
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

/// Finishing a run on a wall clock first moves its clock parts on to the frozen clock, and records
/// the move: the world is scored as it stands at the end, not at the last tick before it.
#[test]
fn a_finished_runs_clock_parts_are_scored_at_the_frozen_clock() {
    run(async {
        let fx = TimeFx::new("frozen", &[]).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(fx.on(&fx.branch, "finish").await.0, 200);
        let evs = records(&fx.fx.state.store)
            .events(&fx.branch)
            .await
            .unwrap();
        let is_end = |e: &&Event| {
            e.kind == EventKind::WorldEvent
                && e.payload["part"] == "clock"
                && e.payload["data"]["reason"] == "end"
        };
        let end = evs.iter().find(is_end).expect("the move to the end");
        let frozen = end.payload["clock_ms"].as_i64().unwrap();
        let lines = fx.lines();
        let last = lines.iter().rev().find(|l| l[0] == "advance").unwrap();
        assert_eq!(last[2].parse::<i64>().unwrap(), frozen);
        let check = evs.iter().find(|e| e.kind == EventKind::Check).unwrap();
        assert!(end.seq < check.seq, "moved before it was scored");
    })
}

/// The clock `box/clock` shows in the rows of `branch` at `at` (a step's seq, or `final`).
async fn clock_row(fx: &AppFx, branch: &str, at: &str) -> i64 {
    let path = format!("/api/branches/{branch}/rows?seq={at}&keys=box/clock&part=box");
    let (status, v) = fx.get(&path).await;
    assert_eq!(status, 200, "{v}");
    v["rows"]["box/clock"]["ms"].as_i64().unwrap()
}

/// Rows show a clock part as it stands at each step, not as its last save left it: a world event
/// between saves reads the part at that event's clock, and `seq=final` reads the final state.
#[test]
fn rows_show_a_clock_part_at_the_steps_clock_and_at_the_end() {
    run(async {
        let fx = TimeFx::with_part("rows", &["--event-every", "--clock-doc"], &[]).await;
        let ctx = fx.fx.state.live.get(&fx.branch).unwrap();
        let exec = shaked::exec::ToolExec::new(fx.fx.state.store.clone());
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(exec.run(&ctx, "planner", "poke", json!({}), None).await["error"].is_null());
        tokio::time::sleep(Duration::from_millis(600)).await;
        let evs = records(&fx.fx.state.store)
            .events(&fx.branch)
            .await
            .unwrap();
        let poke = evs.iter().rfind(|e| e.kind == EventKind::ToolCall).unwrap();
        let clock = |e: &Event| e.payload["clock_ms"].as_i64().unwrap();
        let seq = poke.seq.to_string();
        assert_eq!(clock_row(&fx.fx, &fx.branch, &seq).await, clock(poke));
        let later = (evs.iter().rev())
            .find(|e| e.kind == EventKind::WorldEvent && clock(e) > clock(poke))
            .expect("a tick's event after the poke");
        let at = later.seq.to_string();
        assert_eq!(clock_row(&fx.fx, &fx.branch, &at).await, clock(later));
        assert_eq!(fx.on(&fx.branch, "finish").await.0, 200);
        let evs = records(&fx.fx.state.store)
            .events(&fx.branch)
            .await
            .unwrap();
        let end = (evs.iter().rev())
            .find(|e| e.kind == EventKind::WorldEvent && e.payload["data"]["reason"] == "end")
            .unwrap();
        assert_eq!(clock_row(&fx.fx, &fx.branch, "final").await, clock(end));
    })
}

/// A project whose world `toy` is run by `toybench` and has one view `clock`: a figure of the
/// clock part's clock and a board of its docs.
fn view_project(tag: &str) -> tempfile::TempDir {
    let root = tempfile::Builder::new().prefix(tag).tempdir().unwrap();
    let world = root.path().join("worlds/toy");
    std::fs::create_dir_all(world.join("views")).unwrap();
    let project = "format: shakedown-project/1\nname: P\n";
    std::fs::write(root.path().join("shakedown.yaml"), project).unwrap();
    let file =
        "format: shakedown-world/3\nlabel: Toy\npackage: toybench\nviews: [views/clock.yaml]\n";
    std::fs::write(world.join("world.yaml"), file).unwrap();
    let view = r#"format: shakedown-view/1
label: Clock
part: box
panels:
  now: {kind: kpis, items: [{label: Clock, value: {key: box/clock, field: ms}, format: int}]}
  docs: {kind: board, data: {find: "box/*"}, columns: [{field: n, label: N}]}
  feed: {kind: feed, kinds: [alarm]}
screens:
  - {id: main, label: Main, areas: ["now feed", "docs feed"]}
"#;
    std::fs::write(world.join("views/clock.yaml"), view).unwrap();
    root
}

/// The clock figure of the view `clock` at `at`.
async fn clock_figure(fx: &AppFx, branch: &str, at: &str) -> (Value, Value) {
    let path = format!("/api/branches/{branch}/world-views/clock?screen=main&at={at}");
    let (status, v) = fx.get(&path).await;
    assert_eq!(status, 200, "{v}");
    (
        v["panels"]["now"]["items"][0]["value"].clone(),
        v["at"].clone(),
    )
}

/// The clock of the branch's move to its frozen clock.
async fn end_clock(fx: &TimeFx) -> Value {
    let evs = records(&fx.fx.state.store)
        .events(&fx.branch)
        .await
        .unwrap();
    let end = (evs.iter().rev())
        .find(|e| e.kind == EventKind::WorldEvent && e.payload["data"]["reason"] == "end")
        .expect("the move to the end");
    end.payload["clock_ms"].clone()
}

/// The branch's view list names its project world and the view `clock`.
async fn assert_listed(fx: &TimeFx) {
    let (status, list) = fx
        .fx
        .get(&format!("/api/branches/{}/world-views", fx.branch))
        .await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(
        (&list["world"], &list["views"][0]["id"]),
        (&json!("toy"), &json!("clock"))
    );
}

/// A live view of an ended branch is a conflict, and a view its world lacks is not found.
async fn assert_refused(fx: &TimeFx) {
    let path = format!("/api/branches/{}/world-views/clock?at=live", fx.branch);
    assert_eq!(fx.fx.get(&path).await.0, 409);
    let path = format!("/api/branches/{}/world-views/nope", fx.branch);
    assert_eq!(fx.fx.get(&path).await.0, 404);
}

/// A world's view lists from its project world, and evaluates live, at a step (the clock part
/// moved on to the step's clock) and at the end; live once the run ended is a conflict.
#[test]
fn a_worlds_view_evaluates_live_at_a_step_and_at_the_end() {
    run(async {
        let project = view_project("views");
        let part = ["--event-every", "--clock-doc"];
        let fx = TimeFx::build("views", &part, &[], Some(project.path().to_path_buf())).await;
        assert_listed(&fx).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let (live, at) = clock_figure(&fx.fx, &fx.branch, "live").await;
        assert!(live.as_i64().unwrap() > 0 && at["point"] == "live", "{at}");
        let evs = records(&fx.fx.state.store)
            .events(&fx.branch)
            .await
            .unwrap();
        let tick = (evs.iter().rev())
            .find(|e| e.kind == EventKind::WorldEvent)
            .unwrap();
        let (value, at) = clock_figure(&fx.fx, &fx.branch, &tick.seq.to_string()).await;
        assert_eq!(
            (&value, &at["clock_ms"]),
            (&tick.payload["clock_ms"], &tick.payload["clock_ms"])
        );
        assert_eq!(fx.on(&fx.branch, "finish").await.0, 200);
        let (value, at) = clock_figure(&fx.fx, &fx.branch, "final").await;
        assert_eq!(
            (value, &at["point"]),
            (end_clock(&fx).await, &json!("final"))
        );
        assert_refused(&fx).await;
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
        SteppedFx::costing(tag, part_args, step_cost(0, 120_000)).await
    }

    /// `new` with calls costing `cost`.
    async fn costing(tag: &str, part_args: &[&str], cost: shaked::clock::StepCost) -> SteppedFx {
        let root = packages::toy_world(tag, part_args, &[]);
        let saves =
            std::env::temp_dir().join(format!("shaked-stepped-{tag}-{}", std::process::id()));
        let (store, clean) = support::app::meta_store("stepped", tag).await;
        let ctx = stepped_branch(&root, &saves, &store, cost).await;
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

/// What reads and writes cost a stepped clock.
fn step_cost(read_ms: i64, write_ms: i64) -> shaked::clock::StepCost {
    shaked::clock::StepCost {
        read_ms,
        write_ms,
        tools: Default::default(),
    }
}

/// A branch of `toybench` from `root` (saves in `saves`) on a stepped clock whose calls cost
/// `cost`, recorded in `store`.
async fn stepped_branch(
    root: &packages::ToyRoot,
    saves: &std::path::Path,
    store: &shaked::store::Store,
    cost: shaked::clock::StepCost,
) -> shaked::exec::BranchCtx {
    std::fs::create_dir_all(saves).unwrap();
    let registry = parts::Registry::scan(std::slice::from_ref(&root.0)).unwrap();
    let hub = parts::Hub::new(registry, packages::bin_dir(), saves.to_path_buf());
    let hub = std::sync::Arc::new(hub);
    let team = Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![AgentSpec::new("planner", AgentKind::Open)],
    };
    let run_id = records(store).create_run("t1", &team).await.unwrap();
    let branch = (records(store).create_branch(&run_id, None, &team, &json!([]), None))
        .await
        .unwrap();
    let bench = hub.bench_describe("toybench").await.unwrap();
    let from = shaked::world::WorldFrom::Base;
    let world = shaked::world::World::start(hub.clone(), &bench, &branch, 1, from).await;
    let world = std::sync::Arc::new(world.unwrap());
    shaked::exec::BranchCtx::new(branch, world, team, "t1".into())
        .with_clock(shaked::clock::BranchClock::stepped(0, cost))
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

/// An upstream for `toy.test`: `GET /hello`, `POST /items` and `POST /graphql`, which says it only
/// read.
async fn http_upstream() -> String {
    let graphql = || async { ([("x-shakedown-effect", "read")], "{}") };
    let app = axum::Router::new()
        .route("/hello", axum::routing::get(|| async { "hi" }))
        .route("/items", axum::routing::post(|| async { "made" }))
        .route("/graphql", axum::routing::post(graphql));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

/// A `method` request for `http://toy.test<path>` as an agent's client sends it.
fn request_to(method: &str, path: &str) -> shaked::exec::HttpCall {
    shaked::exec::HttpCall {
        scheme: "http",
        method: method.to_string(),
        host: "toy.test".to_string(),
        port: 80,
        path_and_query: path.to_string(),
        headers: vec![],
        body: bytes::Bytes::new(),
    }
}

/// On a stepped clock a request through the proxy costs world time as a tool call does, by its
/// effect: a GET reads, a POST writes, and the part's `X-Shakedown-Effect` has the last word.
#[test]
fn proxy_requests_charge_the_stepped_clock_by_effect() {
    run(async {
        let route = format!("toy.test={}", http_upstream().await);
        let part = ["--uses-clock", "--http-route", route.as_str()];
        let fx = SteppedFx::costing("http", &part, step_cost(30_000, 120_000)).await;
        let mut clocks = vec![];
        for (method, path) in [("GET", "/hello"), ("POST", "/items"), ("POST", "/graphql")] {
            let answer = (fx.exec)
                .http(&fx.ctx, Some("planner"), request_to(method, path))
                .await;
            assert_eq!(answer.status, 200, "{method} {path}");
            clocks.push(fx.ctx.clock.now_ms());
        }
        assert_eq!(clocks, [30_000, 150_000, 180_000]);
        assert_eq!(fx.call_clocks().await, [0, 30_000, 150_000]);
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

/// The providers: only the scripted server `pod` at `base`.
fn pod(base: &str) -> shaked::providers::ProviderConfig {
    use shaked::providers::{Endpoint, LocalEndpoint, ProviderConfig};
    let mut providers = ProviderConfig::default();
    providers.locals.push(LocalEndpoint {
        name: "pod".into(),
        endpoint: Endpoint::new(base, None),
        rate: None,
    });
    providers.default = "local:pod".into();
    providers
}

/// A dispatch run of the toy world on a stepped clock (`clock`), its lead an API-mode agent on a
/// scripted server answering `answers` in order: the ended branch's events.
async fn stepped_run(
    tag: &str,
    part_args: &[&str],
    clock: Value,
    answers: Vec<Value>,
) -> (AppFx, Vec<Event>) {
    let llm = support::fake_llm::fake_llm(answers.into_iter().map(|a| (200, a)).collect()).await;
    let root = packages::toy_world(tag, part_args, &["--clock-start", "2026-03-10T06:00:00"]);
    let fx = AppFx::idle("stepped", tag)
        .root(root)
        .bench("toybench")
        .providers(pod(&llm.base))
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
    let evs = ended(&fx, v["branch_id"].as_str().unwrap()).await;
    (fx, evs)
}

/// The events of `branch` once it has ended.
async fn ended(fx: &AppFx, branch: &str) -> Vec<Event> {
    for _ in 0..600 {
        let (_, b) = fx.get(&format!("/api/branches/{branch}")).await;
        if !b["ended_at"].is_null() {
            return records(&fx.state.store).events(branch).await.unwrap();
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

/// A stepped clock needs a lead to wake: other runners refuse it before anything runs.
#[test]
fn a_stepped_run_on_another_runner_is_refused() {
    run(async {
        let root = packages::toy_world("steppedchain", &["--uses-clock"], &[]);
        let fx = AppFx::idle("stepped", "chain")
            .root(root)
            .bench("toybench")
            .start()
            .await;
        let team = json!({"id": "t", "name": "T", "agents": []});
        let scenario = json!({"disruptions": [], "clock": {"write_ms": 120_000}});
        for runner in ["chain", "external"] {
            let body =
                json!({"task_id": "t1", "team": team, "runner": runner, "scenario": scenario});
            let (status, v) = fx.post("/api/runs", body).await;
            assert_eq!(status, 400, "{v}");
            assert_eq!(
                v["error"],
                "stepped clock needs a team with a lead (dispatch runner)"
            );
        }
        assert!(records(&fx.state.store).runs().await.unwrap().is_empty());
    })
}
