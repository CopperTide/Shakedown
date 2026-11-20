//! World time: parts get the world's epoch and clock speed at `start` (a task's `clock_start` read
//! as UTC, else the run's start), forks keep their run's epoch, and the clock route tells a driver
//! the world's time. The run is one an external system drives; no model answers anything here.
mod support;

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
        let evs = fx.fx.state.store.events(&fx.branch).await.unwrap();
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
