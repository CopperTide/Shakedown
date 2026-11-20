//! Experiments: one task in one scenario, 1 to 50 agent setups, each run N times, the runs
//! interleaved setup by setup. Without a model: the setups' agents call a provider nothing
//! answers on (each run ends `infra_error` at its first model call) or one that never answers
//! (the run stays running until it is stopped). What is refused, the order the runs are made in,
//! the shapes the API answers, suites made on their own shown as one-setup experiments, stopping,
//! and an experiment left running when shaked stopped.
mod support;

use std::time::Duration;

use serde_json::{json, Value};
use shaked::{
    model::*,
    providers::ProviderConfig,
    store::{NewExperiment, NewSetup, NewSuite, Store},
};
use support::{app, packages::toy_world, pg, pg::Cleanup, run, AppFx};

/// How long a poll waits for the experiment to get where the test expects.
const WAIT: Duration = Duration::from_secs(60);

/// An app on the toy benchmark (tasks t1 and t2) whose open agents call `upstream`.
async fn setup(tag: &str, upstream: &str) -> AppFx {
    let root = toy_world(tag, &[], &["--scored", "--two-tasks", "--safety"]);
    AppFx::idle("experiments", tag)
        .root(root)
        .bench("toybench")
        .providers(ProviderConfig::with_upstream(upstream))
        .start()
        .await
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

impl AppFx {
    /// Creates an experiment: its detail.
    async fn experiment(&self, body: Value) -> Value {
        let (status, v) = self.post("/api/experiments", body).await;
        assert_eq!(status, 200, "{v}");
        v
    }

    /// The experiment's detail once `done` holds for it.
    async fn until(&self, id: &str, done: impl Fn(&Value) -> bool) -> Value {
        let polls = WAIT.as_millis() / 100;
        for _ in 0..polls {
            let (status, v) = self.get(&format!("/api/experiments/{id}")).await;
            assert_eq!(status, 200, "{v}");
            if done(&v) {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("experiment {id} did not get there");
    }
}

/// A team of one open agent (on whatever provider the app has).
fn team(id: &str) -> Value {
    let agent = json!({"id": "planner", "name": "Planner", "kind": "open", "model": "m",
                       "prompt": "Poke the box."});
    json!({"id": id, "name": format!("Team {id}"), "agents": [agent]})
}

fn setup_of(name: Option<&str>, team_id: &str) -> Value {
    let mut s = json!({"team": team(team_id)});
    if let Some(n) = name {
        s["name"] = json!(n);
    }
    s
}

/// A request that must be refused (400), naming `says` in its error.
async fn refused(fx: &AppFx, body: Value, says: &str) {
    let (status, v) = fx.post("/api/experiments", body).await;
    assert_eq!(status, 400, "{v}");
    let error = v["error"].as_str().unwrap();
    assert!(error.contains(says), "{error} should say {says}");
}

/// Refused: too few or many setups, setups with one name, an external runner, a team that
/// can't run.
async fn refuse_bad_setups(fx: &AppFx) {
    refused(fx, json!({"task": "t1", "setups": []}), "1 to 50 setups").await;
    let too_many: Vec<Value> = (0..51).map(|i| setup_of(None, &format!("t{i}"))).collect();
    refused(
        fx,
        json!({"task": "t1", "setups": too_many}),
        "1 to 50 setups",
    )
    .await;
    let twins = [setup_of(Some("Same"), "a"), setup_of(Some(" Same "), "b")];
    refused(fx, json!({"task": "t1", "setups": twins}), "Same").await;
    let mut external = setup_of(None, "a");
    external["runner"] = json!("external");
    let body = json!({"task": "t1", "setups": [external]});
    refused(fx, body, "an experiment's setups run on shaked's runners").await;
    let empty = json!({"team": {"id": "e", "name": "E", "agents": []}});
    refused(fx, json!({"task": "t1", "setups": [empty]}), "Setup A: ").await;
}

#[test]
fn experiments_refuse_what_they_cannot_run() {
    run(async {
        let fx = setup("xrefuse", app::NOWHERE).await;
        let one = || vec![setup_of(None, "a")];
        refuse_bad_setups(&fx).await;
        refused(&fx, json!({"task": "t9", "setups": one()}), "t9").await;
        for runs in [0, 1001] {
            let body = json!({"task": "t1", "runs": runs, "setups": one()});
            refused(&fx, body, "runs must be from 1 to 1000").await;
        }
        let scenario = json!({"disruptions": [{"id": "flood"}]});
        let body = json!({"task": "t1", "scenario": scenario, "setups": one()});
        refused(&fx, body, "disruption").await;
        let (status, _) = fx.get("/api/experiments/nope").await;
        assert_eq!(status, 404);
        let (status, _) = fx.post("/api/experiments/nope/stop", json!({})).await;
        assert_eq!(status, 404);
        assert_eq!(
            fx.get("/api/experiments").await.1,
            json!([]),
            "nothing was made"
        );
    });
}

/// The detail answers the experiment and a suite report per setup, in setup order.
fn assert_created(v: &Value) {
    let head = (
        &v["name"],
        &v["tasks"],
        &v["runs"],
        &v["legacy"],
        &v["benchmark"],
    );
    let want = json!(["Setup A vs Rules", ["t1"], 2, false, "toybench"]);
    assert_eq!(json!([head.0, head.1, head.2, head.3, head.4]), want, "{v}");
    assert_eq!(v["scenario"], json!({"disruptions": []}), "{v}");
    let setups = v["setups"].as_array().unwrap();
    let names: Vec<&Value> = setups.iter().map(|s| &s["name"]).collect();
    assert_eq!(names, [&json!("Setup A"), &json!("Rules")]);
    for (ord, s) in setups.iter().enumerate() {
        assert_eq!(
            (&s["experiment_id"], &s["setup_ord"]),
            (&v["id"], &json!(ord))
        );
        assert_eq!((&s["tasks"], &s["repeats"]), (&json!(["t1"]), &json!(2)));
        assert_eq!(s["runs"].as_array().unwrap().len(), 2, "{s}");
    }
}

/// Each run's branch with its setup and repeat, in the order the runs were made.
async fn made_in_order(fx: &AppFx, v: &Value) -> Vec<(String, i64)> {
    let mut made = vec![];
    for s in v["setups"].as_array().unwrap() {
        for row in s["runs"].as_array().unwrap() {
            let branch = row["branch_id"].as_str().expect("every run started");
            let b = fx.state.store.branch(branch).await.unwrap();
            let who = (
                s["name"].as_str().unwrap().to_string(),
                row["repeat"].as_i64().unwrap(),
            );
            made.push((b.created_at, who));
        }
    }
    made.sort_by_key(|(at, _)| *at);
    made.into_iter().map(|(_, who)| who).collect()
}

/// Two setups, two runs each: the runs go A1, B1, A2, B2; each suite ends with its runs and the
/// experiment when all are done.
#[test]
fn an_experiment_interleaves_its_setups_runs() {
    run(async {
        let fx = setup("xorder", app::NOWHERE).await;
        let setups = [setup_of(None, "a"), setup_of(Some(" Rules "), "b")];
        let v = fx
            .experiment(json!({"task": "t1", "runs": 2, "setups": setups}))
            .await;
        assert_created(&v);
        let id = v["id"].as_str().unwrap();
        let done = fx.until(id, |v| v["status"] != "running").await;
        assert_eq!(
            (&done["status"], &done["error"]),
            (&json!("finished"), &Value::Null)
        );
        assert!(done["ended_at"].is_string(), "{done}");
        let order = made_in_order(&fx, &done).await;
        let want = [("Setup A", 1), ("Rules", 1), ("Setup A", 2), ("Rules", 2)];
        let want: Vec<(String, i64)> = want.iter().map(|(n, r)| (n.to_string(), *r)).collect();
        assert_eq!(order, want);
        assert_setups_finished(&done);
    });
}

/// Every setup's suite finished and ended with no error; each run ended at its model call.
fn assert_setups_finished(done: &Value) {
    for s in done["setups"].as_array().unwrap() {
        assert_eq!(s["status"], "finished", "{s}");
        assert!(s["ended_at"].is_string() && s["error"].is_null(), "{s}");
        let summary = &s["summary"];
        assert_eq!(
            (&summary["denials"], &summary["flags"]),
            (&json!(0), &json!(0))
        );
        for row in s["runs"].as_array().unwrap() {
            assert_eq!(row["status"], "infra_error", "{row}");
            assert_eq!(row["outcome"]["denials"], 0, "{row}");
        }
    }
}

fn data_team(id: &str) -> Team {
    serde_json::from_value(team(id)).unwrap()
}

/// A suite stored on its own (external, so nothing drives it): its id.
async fn stored_suite(store: &Store) -> String {
    let (t, tasks, scenario) = (data_team("old"), vec!["t1".to_string()], Default::default());
    let suite = NewSuite {
        name: "Old suite",
        team: &t,
        tasks: &tasks,
        repeats: 3,
        runner: Runner::External,
        benchmark: "toybench",
        scenario: &scenario,
        variants: &[],
        max_cost_usd: None,
        connection: None,
    };
    store.create_suite(suite).await.unwrap().id
}

/// A suite stored on its own and then an experiment of two setups stored without being driven:
/// their ids.
async fn stored(store: &Store) -> (String, String) {
    let legacy = stored_suite(store).await;
    let (a, b, scenario) = (data_team("a"), data_team("b"), Default::default());
    let setup = |name, team, runner| NewSetup {
        name,
        team,
        runner,
        connection: None,
    };
    let setups = [
        setup("A", &a, Runner::Chain),
        setup("B", &b, Runner::Concurrent),
    ];
    let new = NewExperiment {
        name: "A vs B",
        task: "t1",
        runs: 1,
        benchmark: "toybench",
        scenario: &scenario,
        setups: &setups,
        max_cost_usd: None,
        seed: 7,
        world_minutes: None,
    };
    let (e, suites) = store.create_experiment(new).await.unwrap();
    assert_eq!(suites.len(), 2);
    (legacy, e.id)
}

/// The list has the experiment (stored last) first, then the suite made on its own.
fn assert_listed(list: &Value, legacy: &str, exp: &str) {
    assert_eq!(list.as_array().unwrap().len(), 2, "{list}");
    let (e, old) = (&list[0], &list[1]);
    assert_experiment_line(e, exp);
    assert_eq!(
        (&old["id"], &old["legacy"], &old["runs"]),
        (&json!(legacy), &json!(true), &json!(3))
    );
    assert_eq!(old["setups"][0]["suite_id"], json!(legacy));
    assert_eq!(old["setups"][0]["runs"], 3);
}

fn assert_experiment_line(e: &Value, exp: &str) {
    assert_eq!(
        (&e["id"], &e["legacy"], &e["status"]),
        (&json!(exp), &json!(false), &json!("running"))
    );
    assert_eq!(
        (&e["tasks"], &e["runs"], &e["name"]),
        (&json!(["t1"]), &json!(1), &json!("A vs B"))
    );
    let line = |s: &Value| {
        json!([
            s["name"],
            s["team_name"],
            s["runner"],
            s["runs"],
            s["ended"],
            s["passed"]
        ])
    };
    let lines: Vec<Value> = e["setups"].as_array().unwrap().iter().map(line).collect();
    assert_eq!(
        lines,
        [
            json!(["A", "Team a", "chain", 1, 0, 0]),
            json!(["B", "Team b", "concurrent", 1, 0, 0])
        ]
    );
}

/// The list shows experiments and suites made on their own (as one-setup experiments), newest
/// first; a legacy suite reads as an experiment but can't be stopped; `/api/suites` is as before.
#[test]
fn experiments_list_with_suites_made_on_their_own() {
    run(async {
        let fx = setup("xlist", app::NOWHERE).await;
        let (legacy, exp) = stored(&fx.state.store).await;
        let (status, list) = fx.get("/api/experiments").await;
        assert_eq!(status, 200, "{list}");
        assert_listed(&list, &legacy, &exp);
        assert_legacy_detail(&fx, &legacy).await;
        let (_, e) = fx.get(&format!("/api/experiments/{exp}")).await;
        let setup_suite = e["setups"][1]["id"].as_str().unwrap();
        let (status, _) = fx.get(&format!("/api/experiments/{setup_suite}")).await;
        assert_eq!(status, 404, "a setup's suite is not an experiment");
        let (_, suites) = fx.get("/api/suites").await;
        assert_eq!(suites.as_array().unwrap().len(), 3);
        let own = suites
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == json!(legacy));
        assert!(own.unwrap().get("experiment_id").is_none(), "{suites}");
    });
}

/// A suite made on its own reads as a one-setup experiment, which can't be stopped.
async fn assert_legacy_detail(fx: &AppFx, legacy: &str) {
    let (status, old) = fx.get(&format!("/api/experiments/{legacy}")).await;
    assert_eq!(status, 200, "{old}");
    assert_eq!(
        (&old["legacy"], &old["name"]),
        (&json!(true), &json!("Old suite"))
    );
    assert_eq!(old["setups"][0]["id"], json!(legacy));
    assert_eq!(old["setups"][0]["runs"].as_array().unwrap().len(), 3);
    let stop = format!("/api/experiments/{legacy}/stop");
    assert_eq!(fx.post(&stop, json!({})).await.0, 409);
}

/// The experiment's first run, once it is running.
async fn first_running(fx: &AppFx, id: &str) -> Value {
    fx.until(id, |v| v["setups"][0]["runs"][0]["status"] == "running")
        .await
}

impl AppFx {
    /// Stops the experiment: its detail.
    async fn stop(&self, id: &str) -> Value {
        let (status, v) = self
            .post(&format!("/api/experiments/{id}/stop"), json!({}))
            .await;
        assert_eq!(status, 200, "{v}");
        v
    }
}

/// After the stop, every suite ended and only the first run ever started.
fn assert_nothing_else_ran(after: &Value) {
    for s in after["setups"].as_array().unwrap() {
        assert!(s["ended_at"].is_string(), "{s}");
        let rows = s["runs"].as_array().unwrap();
        let pending = rows.iter().filter(|r| r["status"] == "pending");
        let want = if s["setup_ord"] == 0 { 2 } else { 3 };
        assert_eq!(pending.count(), want, "no other run started: {s}");
    }
}

/// Stopping stops the running run and starts no other; the suites end; stopping again changes
/// nothing.
#[test]
fn stopping_an_experiment_stops_its_running_run() {
    run(async {
        let fx = setup("xstop", &silent_provider().await).await;
        let setups = [setup_of(None, "a"), setup_of(None, "b")];
        let v = fx
            .experiment(json!({"task": "t2", "runs": 3, "setups": setups}))
            .await;
        let id = v["id"].as_str().unwrap();
        first_running(&fx, id).await;
        let s = fx.stop(id).await;
        let said = (&s["status"], &s["error"]);
        assert_eq!(said, (&json!("stopped"), &json!("stopped")));
        let done = fx
            .until(id, |v| v["setups"][0]["runs"][0]["status"] == "stopped")
            .await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_nothing_else_ran(&fx.get(&format!("/api/experiments/{id}")).await.1);
        let again = fx.stop(id).await;
        let said = (&again["ended_at"], &again["error"]);
        assert_eq!(said, (&done["ended_at"], &json!("stopped")));
    });
}

/// More than the old limits of 4 setups and 100 runs: five setups of 101 runs each are made (named
/// A to E), then stopped.
#[test]
fn an_experiment_takes_more_than_four_setups_and_a_hundred_runs() {
    run(async {
        let fx = setup("xmany", &silent_provider().await).await;
        let setups: Vec<Value> = (0..5).map(|i| setup_of(None, &format!("t{i}"))).collect();
        let v = fx
            .experiment(json!({"task": "t2", "runs": 101, "setups": setups}))
            .await;
        let made = v["setups"].as_array().unwrap();
        let names: Vec<&str> = made.iter().map(|s| s["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            ["Setup A", "Setup B", "Setup C", "Setup D", "Setup E"]
        );
        assert!(made.iter().all(|s| s["repeats"] == 101), "{v}");
        let id = v["id"].as_str().unwrap();
        first_running(&fx, id).await;
        assert_eq!(fx.stop(id).await["status"], "stopped");
    });
}

/// A fresh metadata database's name, dropped with the returned guard.
fn fresh_db() -> (String, Cleanup) {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let name = format!("arena_test_experiments_restart_{}", &id[..8]);
    let clean = Cleanup::new();
    clean.add(&name);
    (name, clean)
}

#[test]
fn an_experiment_left_running_is_stopped_when_shaked_starts_again() {
    run(async {
        let (name, _clean) = fresh_db();
        let store = Store::open_named(&pg::admin(), &name).await.unwrap();
        let (_, exp) = stored(&store).await;
        drop(store);
        let store = Store::open_named(&pg::admin(), &name).await.unwrap();
        let e = store.experiment(&exp).await.unwrap().unwrap();
        assert!(e.error.unwrap().contains("shaked stopped") && e.ended_at.is_some());
        for s in store.experiment_suites(&exp).await.unwrap() {
            assert!(s.ended_at.is_some() && s.error.is_some(), "{s:?}");
        }
    });
}

// ---------------------------------------------------------------- paired seeds and the window

/// Runs `body` as an experiment to its end: its detail.
async fn finished(fx: &AppFx, body: Value) -> Value {
    let v = fx.experiment(body).await;
    let id = v["id"].as_str().unwrap().to_string();
    fx.until(&id, |v| v["status"] != "running").await
}

/// The `RunStart` of each run of setup `i`, in repeat order.
async fn run_starts(fx: &AppFx, done: &Value, i: usize) -> Vec<RunStartPayload> {
    let mut out = vec![];
    for row in done["setups"][i]["runs"].as_array().unwrap() {
        let branch = row["branch_id"].as_str().unwrap();
        let evs = fx.state.store.events(branch).await.unwrap();
        out.push(RunStartPayload::of(&evs).expect("a RunStart"));
    }
    out
}

fn seeds(starts: &[RunStartPayload]) -> Vec<u64> {
    starts.iter().map(|s| s.seed).collect()
}

/// Run i of every setup has one seed, the repeats' seeds differ, and each run row shows its seed.
async fn assert_paired(fx: &AppFx, done: &Value) {
    let (a, b) = (run_starts(fx, done, 0).await, run_starts(fx, done, 1).await);
    assert_eq!(seeds(&a), seeds(&b), "run i of A and of B share a seed");
    let mut distinct = seeds(&a);
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 3, "{:?}", seeds(&a));
    let rows = done["setups"][0]["runs"].as_array().unwrap();
    let shown: Vec<u64> = rows.iter().map(|r| r["seed"].as_u64().unwrap()).collect();
    assert_eq!(shown, seeds(&a));
}

#[test]
fn setups_share_a_seed_per_repeat() {
    run(async {
        let fx = setup("xseeds", app::NOWHERE).await;
        let setups = [setup_of(None, "a"), setup_of(None, "b")];
        let done = finished(
            &fx,
            json!({"task": "t1", "runs": 3, "seed": 7, "setups": setups}),
        )
        .await;
        assert_eq!(done["seed"], 7, "{done}");
        assert_paired(&fx, &done).await;
    });
}

#[test]
fn an_experiment_passes_its_window_to_every_run() {
    run(async {
        let fx = setup("xwindow", app::NOWHERE).await;
        let setups = [setup_of(None, "a"), setup_of(None, "b")];
        let body = json!({"task": "t1", "runs": 2, "world_minutes": 0.05, "max_cost_usd": 1.5, "setups": setups});
        let done = finished(&fx, body).await;
        for i in 0..2 {
            for s in run_starts(&fx, &done, i).await {
                assert_eq!(s.world_minutes, Some(0.05));
            }
        }
        let head = (&done["world_minutes"], &done["max_cost_usd"]);
        assert_eq!(head, (&json!(0.05), &json!(1.5)), "{done}");
        assert!(done["seed"].is_u64(), "{done}");
    });
}

#[test]
fn an_experiment_without_a_seed_draws_one_and_records_it() {
    run(async {
        let fx = setup("xdrawn", app::NOWHERE).await;
        let setups = [setup_of(None, "a"), setup_of(None, "b")];
        let done = finished(&fx, json!({"task": "t1", "runs": 3, "setups": setups})).await;
        assert!(done["seed"].as_u64().is_some_and(|s| s < 1 << 53), "{done}");
        assert_paired(&fx, &done).await;
        let listed = fx.get("/api/experiments").await.1;
        assert_eq!(listed[0]["seed"], done["seed"]);
    });
}

#[test]
fn an_experiments_seed_and_window_are_checked() {
    run(async {
        let fx = setup("xseedbad", app::NOWHERE).await;
        let one = [setup_of(None, "a")];
        refused(
            &fx,
            json!({"task": "t1", "seed": 1u64 << 53, "setups": one}),
            "seed must be at most 2^53 - 1",
        )
        .await;
        let body = json!({"task": "t1", "world_minutes": 0, "setups": one});
        refused(
            &fx,
            body,
            "world_minutes must be from 0 to 1440 minutes of world time",
        )
        .await;
    });
}
