//! Analytics against fixture runs in a fresh store: each ended branch's figures, worked out by the
//! analytics job, then the summary's aggregates in SQL (pass rate, cost per pass, the leaderboard,
//! the points), its filters, its staleness when a branch ends again, and the `analytics.summary`
//! operation over HTTP.
mod support;

use serde_json::{json, Value};
use shaked::{
    analytics::{self, Filter},
    model::*,
    ops::{self, Access},
    store::{Store, Summary},
};
use support::{pg, records, run, team_of, AppFx};

/// One fixture run: who ran it, on what, how it ended and what its one model call cost.
struct Fx {
    setup: &'static str,
    model: &'static str,
    scenario: &'static str,
    status: BranchStatus,
    cost: f64,
}

const fn fx(setup: &'static str, model: &'static str, status: BranchStatus, cost: f64) -> Fx {
    Fx {
        setup,
        model,
        scenario: "refund",
        status,
        cost,
    }
}

/// Alpha (m1) passes 2 of 3 refunds at $0.50 a run; beta (m2) passes its 1 judged run at $2, plus
/// a stopped run ($1) and a platform failure ($0) the pass rate leaves out; gamma (m1) fails an
/// exchange at $0.25.
const RUNS: [Fx; 7] = [
    fx("alpha", "m1", BranchStatus::Passed, 0.5),
    fx("alpha", "m1", BranchStatus::Passed, 0.5),
    fx("alpha", "m1", BranchStatus::Failed, 0.5),
    fx("beta", "m2", BranchStatus::Passed, 2.0),
    fx("beta", "m2", BranchStatus::Stopped, 1.0),
    fx("beta", "m2", BranchStatus::InfraError, 0.0),
    Fx {
        scenario: "exchange",
        ..fx("gamma", "m1", BranchStatus::Failed, 0.25)
    },
];

fn team(setup: &str, model: &str) -> Team {
    let mut t = team_of(vec![AgentSpec::new("a", AgentKind::Open).with_name("A")]);
    t.name = setup.into();
    t.agents[0].model = Some(model.into());
    t
}

/// Records `fx`'s steps on branch `b` and ends it.
async fn steps(store: &Store, b: &str, fx: &Fx) {
    let s = records(store);
    let usage = json!({"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150});
    let steps = [
        (
            None,
            EventKind::RunStart,
            json!({"task_id": fx.scenario, "benchmark": "retail"}),
        ),
        (
            Some("a"),
            EventKind::AgentStart,
            json!({"name": "A", "kind": "open", "model": fx.model}),
        ),
        (
            Some("a"),
            EventKind::ModelCall,
            json!({"request": {"model": fx.model, "messages": [{"role": "user", "content": "hi"}]},
                   "response": {"choices": []}, "usage": usage, "cost_usd": fx.cost}),
        ),
    ];
    for (agent, kind, payload) in steps {
        s.append(b, agent, kind, payload).await.unwrap();
    }
    s.set_status(b, fx.status, None).await.unwrap();
}

/// Records `fx` as a run with one ended branch: `(run id, branch id)`.
async fn record(store: &Store, fx: &Fx) -> (String, String) {
    let s = records(store);
    let t = team(fx.setup, fx.model);
    let run = s.create_run(fx.scenario, &t).await.unwrap();
    let b = s
        .create_branch(&run, None, &t, &json!([]), None)
        .await
        .unwrap();
    steps(store, &b, fx).await;
    (run, b)
}

/// Records [`RUNS`], and a rerun of the first that passed at $9 (reruns are left out): the ids.
async fn record_all(store: &Store) -> Vec<(String, String)> {
    let mut ids = vec![];
    for f in &RUNS {
        ids.push(record(store, f).await);
    }
    let (run, parent) = &ids[0];
    let t = team("alpha", "m1");
    let s = records(store);
    let fork = s
        .create_branch(run, Some((parent, 1)), &t, &json!([]), None)
        .await
        .unwrap();
    steps(store, &fork, &fx("alpha", "m1", BranchStatus::Passed, 9.0)).await;
    ids
}

/// A fresh store with every fixture recorded and its figures worked out.
async fn filled(tag: &str) -> (Store, pg::Cleanup, Vec<(String, String)>) {
    let (_db, store, clean) = pg::fresh_store(tag).await;
    let ids = record_all(&store).await;
    assert_eq!(analytics::fill(&store, 100).await.unwrap(), RUNS.len() + 1);
    (store, clean, ids)
}

async fn summary(store: &Store, v: Value) -> Summary {
    let q = serde_json::from_value::<Filter>(v)
        .unwrap()
        .resolve()
        .unwrap();
    records(store).analytics_summary(&q).await.unwrap()
}

fn close(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 1e-9)
}

#[test]
fn totals_count_first_branches_and_leave_stopped_and_platform_failures_out_of_the_pass_rate() {
    run(async {
        let (store, _clean, _) = filled("analytics_totals").await;
        let s = summary(&store, json!({})).await;
        let t = &s.totals;
        assert_eq!((t.runs, t.judged, t.passed), (7, 5, 3));
        assert!(close(t.pass_rate, 0.6));
        assert!(close(Some(t.cost_usd), 4.75), "the $9 rerun is left out");
        assert!(close(t.cost_per_pass, 4.75 / 3.0));
        assert!(close(t.cost_per_run, 4.75 / 7.0));
        assert_eq!((t.tokens, t.uncosted), (7 * 150, 0));
        assert!(t.median_seconds.is_some());
        assert_eq!((s.pending.unprocessed, s.pending.running), (0, 0));
        assert_eq!((s.group, s.bucket), ("setup", "day"));
        let passes: i64 = s.series.iter().map(|p| p.passed).sum();
        assert_eq!(passes, 3);
    });
}

#[test]
fn the_leaderboard_and_points_give_pass_rate_cost_per_run_and_cost_per_pass() {
    run(async {
        let (store, _clean, _) = filled("analytics_board").await;
        let s = summary(&store, json!({})).await;
        let row = |setup: &str| s.leaderboard.iter().find(|r| r.setup == setup).unwrap();
        let alpha = row("alpha");
        assert_eq!(
            (alpha.world.as_str(), alpha.scenario.as_str()),
            ("retail", "refund")
        );
        assert_eq!((alpha.runs, alpha.judged, alpha.passed), (3, 3, 2));
        assert!(close(alpha.pass_rate, 2.0 / 3.0));
        assert!(close(alpha.cost_per_run, 0.5));
        assert!(close(alpha.cost_per_pass, 0.75), "$1.50 for 2 passes");
        let beta = row("beta");
        assert_eq!((beta.runs, beta.judged, beta.passed), (3, 1, 1));
        assert!(close(beta.pass_rate, 1.0));
        assert!(close(beta.cost_per_pass, 3.0));
        let gamma = row("gamma");
        assert_eq!((gamma.scenario.as_str(), gamma.passed), ("exchange", 0));
        assert_eq!(gamma.cost_per_pass, None);
        assert_eq!(s.leaderboard.len(), 3);

        let m = summary(&store, json!({"group": "model"})).await;
        let groups: Vec<_> = m
            .points
            .iter()
            .map(|p| (p.group.as_str(), p.runs, p.passed))
            .collect();
        assert_eq!(groups, [("m1", 4, 2), ("m2", 3, 1)]);
        assert!(close(m.points[0].pass_rate, 0.5));
        assert_eq!(m.points[0].tokens, 4 * 150);
        assert!(m.series.iter().all(|p| p.group == "m1" || p.group == "m2"));
    });
}

#[test]
fn every_filter_narrows_the_runs_and_the_options_stay_whole() {
    run(async {
        let (store, _clean, _) = filled("analytics_filters").await;
        let runs = async |v: Value| summary(&store, v).await.totals.runs;
        assert_eq!(runs(json!({"setup": "alpha"})).await, 3);
        assert_eq!(runs(json!({"setup": "alpha,gamma"})).await, 4);
        assert_eq!(runs(json!({"model": "m2"})).await, 3);
        assert_eq!(runs(json!({"scenario": "exchange"})).await, 1);
        assert_eq!(runs(json!({"world": "retail"})).await, 7);
        assert_eq!(runs(json!({"world": "factory"})).await, 0);
        assert_eq!(runs(json!({"status": "passed"})).await, 3);
        assert_eq!(runs(json!({"from": "2099-01-01T00:00:00Z"})).await, 0);
        assert_eq!(runs(json!({"to": "2000-01-01T00:00:00Z"})).await, 0);
        assert_eq!(
            runs(json!({"from": "2000-01-01T00:00:00Z", "to": "2099-01-01T00:00:00Z"})).await,
            7
        );

        let s = summary(&store, json!({"setup": "alpha"})).await;
        assert_eq!(s.options.setup, ["alpha", "beta", "gamma"]);
        assert_eq!(s.options.model, ["m1", "m2"]);
        assert_eq!(s.options.scenario, ["exchange", "refund"]);
        assert_eq!(s.options.world, ["retail"]);
        assert_eq!(
            s.options.status,
            ["failed", "infra_error", "passed", "stopped"]
        );
        let empty = summary(&store, json!({"world": "factory"})).await;
        assert_eq!(empty.totals.pass_rate, None);
        assert!(empty.series.is_empty() && empty.leaderboard.is_empty());
    });
}

#[test]
fn figures_are_worked_out_once_and_again_when_a_branch_ends_again() {
    run(async {
        let (_db, store, _clean) = pg::fresh_store("analytics_fill").await;
        let (_, b) = record(&store, &RUNS[0]).await;
        let s = summary(&store, json!({})).await;
        assert_eq!((s.totals.runs, s.pending.unprocessed), (0, 1));
        assert_eq!(analytics::fill(&store, 100).await.unwrap(), 1);
        assert_eq!(analytics::fill(&store, 100).await.unwrap(), 0, "done once");
        assert_eq!(summary(&store, json!({})).await.totals.passed, 1);

        records(&store)
            .set_status(&b, BranchStatus::Failed, None)
            .await
            .unwrap();
        let s = summary(&store, json!({})).await;
        assert_eq!((s.totals.runs, s.pending.unprocessed), (0, 1), "stale");
        assert_eq!(analytics::fill(&store, 100).await.unwrap(), 1);
        let t = summary(&store, json!({})).await.totals;
        assert_eq!((t.runs, t.passed), (1, 0));
    });
}

#[test]
fn the_summary_operation_reads_as_the_caller_over_http() {
    run(async {
        let op = ops::find("analytics.summary").unwrap();
        assert_eq!((op.access, op.method), (Access::Read, "GET"));
        let fx = AppFx::idle("analytics", "op").start().await;
        record_all(&fx.state.store).await;
        analytics::fill(&fx.state.store, 100).await.unwrap();

        let (code, s) = fx
            .get("/api/analytics/summary?setup=alpha,beta&group=model&status=passed,failed")
            .await;
        assert_eq!(code, 200, "{s}");
        assert_eq!(s["totals"]["runs"], 4);
        assert_eq!(s["group"], "model");
        assert_eq!(s["points"][0]["group"], "m1");
        assert_eq!(s["leaderboard"].as_array().unwrap().len(), 2);

        let (code, err) = fx.get("/api/analytics/summary?group=colour").await;
        assert_eq!(code, 400, "{err}");
        let (code, _) = fx.get("/api/analytics/summary?from=yesterday").await;
        assert_eq!(code, 400);
    });
}

/// Replace only cached figures, retaining the real record's identity and freshness marker.
async fn cached(store: &Store, branch: &str, fields: Value) {
    let rec = store.stats_record(branch).await.unwrap().unwrap();
    let stats = analytics::stats_of(&rec.branch, &rec.task_id, &rec.events);
    let mut value = serde_json::to_value(stats).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    assert!(store
        .put_stats(branch, rec.branch.ended_at.unwrap(), &value)
        .await
        .unwrap());
}

#[test]
fn cost_series_distinguishes_unknown_zero_and_partial_known_cost() {
    run(async {
        let (store, _clean, ids) = filled("analytics_cost_series").await;
        cached(&store, &ids[0].1, json!({"cost_usd":null})).await;
        cached(&store, &ids[6].1, json!({"cost_usd":null})).await;
        let s = summary(&store, json!({})).await;
        assert_eq!(s.series.iter().map(|p| p.runs).sum::<i64>(), 7);
        assert_eq!(s.series.iter().map(|p| p.tokens).sum::<i64>(), 1050);
        assert_eq!(s.series.iter().map(|p| p.uncosted).sum::<i64>(), 2);
        assert!(close(
            Some(s.series.iter().filter_map(|p| p.cost_usd).sum()),
            4.0
        ));
        let unknown = summary(&store, json!({"setup":"gamma"})).await;
        assert_eq!(unknown.series[0].cost_usd, None);
        assert_eq!(unknown.series[0].uncosted, 1);
        let zero = summary(&store, json!({"status":"infra_error"})).await;
        assert_eq!(zero.series[0].cost_usd, Some(0.0));
        assert_eq!(zero.series[0].uncosted, 0);
        let partial = summary(&store, json!({"setup":"alpha"})).await;
        assert!(close(partial.series[0].cost_usd, 1.0));
        assert_eq!(partial.series[0].uncosted, 1);
    });
}

async fn duration_records(store: &Store) {
    for seconds in [0.0, 9.99, 10.0, 30.0, 60.0, 300.0, 900.0, 3600.0] {
        let status = if seconds == 3600.0 {
            BranchStatus::BudgetExceeded
        } else {
            BranchStatus::Passed
        };
        let (_, b) = record(store, &fx("alpha", "m1", status, 0.5)).await;
        cached(store, &b, json!({"seconds":seconds})).await;
    }
    for status in [
        BranchStatus::Stopped,
        BranchStatus::InfraError,
        BranchStatus::Interrupted,
    ] {
        let (_, b) = record(store, &fx("beta", "m2", status, 0.0)).await;
        cached(store, &b, json!({"seconds":10.0})).await;
    }
}

#[test]
fn duration_bins_have_exact_half_open_boundaries_and_only_judged_runs() {
    run(async {
        let (_db, store, _clean) = pg::fresh_store("analytics_duration").await;
        duration_records(&store).await;
        let s = summary(&store, json!({})).await;
        assert_eq!(
            s.duration.iter().map(|b| b.runs).collect::<Vec<_>>(),
            [2, 1, 1, 1, 1, 1, 1]
        );
        assert_eq!(
            s.duration
                .iter()
                .map(|b| b.lower_seconds)
                .collect::<Vec<_>>(),
            [0.0, 10.0, 30.0, 60.0, 300.0, 900.0, 3600.0]
        );
        assert_eq!(s.duration.last().unwrap().upper_seconds, None);
        assert_eq!(s.duration[0].upper_seconds, Some(10.0));
        assert_eq!(
            s.duration.iter().map(|b| b.runs).sum::<i64>(),
            s.totals.judged
        );
        let excluded = summary(&store, json!({"setup":"beta"})).await;
        assert!(excluded.duration.iter().all(|b| b.runs == 0));
    });
}

async fn failing_events(store: &Store, b: &str) {
    let s = records(store);
    for payload in [
        json!({"checks":[{"name":"superseded","passed":false}]}),
        json!({"checks":[{"name":"safety","passed":false},{"name":"safety","passed":false},
                         {"name":"success","passed":true}]}),
    ] {
        s.append(b, None, EventKind::Check, payload).await.unwrap();
    }
    for _ in 0..2 {
        s.append(
            b,
            Some("a"),
            EventKind::ToolCall,
            json!({"name":"book", "error":"book refused: private customer record"}),
        )
        .await
        .unwrap();
    }
}

async fn failure_records(store: &Store) {
    let ids = record_all(store).await;
    failing_events(store, &ids[2].1).await;
    failing_events(store, &ids[6].1).await;
    let t = team("alpha", "m1");
    let fork = records(store)
        .create_branch(&ids[0].0, Some((&ids[0].1, 1)), &t, &json!([]), None)
        .await
        .unwrap();
    steps(store, &fork, &RUNS[2]).await;
    failing_events(store, &fork).await;
    analytics::fill(store, 100).await.unwrap();
}

fn assert_failures(s: &Summary) {
    let checks = &s.failures.checks;
    let gates = &s.failures.gates;
    assert_eq!(checks.len(), 1);
    assert_eq!((checks[0].name.as_str(), checks[0].runs), ("safety", 2));
    assert_eq!(gates.len(), 1);
    assert_eq!((gates[0].name.as_str(), gates[0].runs), ("book", 2));
    let reasons: Vec<_> = s
        .failures
        .reasons
        .iter()
        .map(|r| (r.name.as_str(), r.runs))
        .collect();
    assert_eq!(
        reasons,
        [("Failed", 2), ("Platform failure", 1), ("Stopped", 1)]
    );
}

#[test]
fn failures_count_affected_first_branches_with_final_check_names_and_filters() {
    run(async {
        let (_db, store, _clean) = pg::fresh_store("analytics_failures").await;
        failure_records(&store).await;
        let s = summary(&store, json!({})).await;
        assert_failures(&s);
        let only = summary(
            &store,
            json!({"scenario":"exchange","model":"m1","world":"retail"}),
        )
        .await;
        assert_eq!(only.failures.checks[0].runs, 1);
        assert_eq!(only.failures.gates[0].runs, 1);
        assert_eq!(only.failures.reasons[0].runs, 1);
        let empty = summary(&store, json!({"world":"factory"})).await;
        assert!(
            empty.failures.checks.is_empty()
                && empty.failures.gates.is_empty()
                && empty.failures.reasons.is_empty()
        );
        assert_eq!(empty.duration.len(), 7);
        assert!(empty.duration.iter().all(|b| b.runs == 0));
    });
}

#[test]
fn old_stats_are_backfilled_for_the_new_failure_shape() {
    run(async {
        let (_db, store, _clean) = pg::fresh_store("analytics_version").await;
        let (_, b) = record(&store, &RUNS[2]).await;
        failing_events(&store, &b).await;
        cached(&store, &b, json!({"v":1})).await;
        assert_eq!(summary(&store, json!({})).await.pending.unprocessed, 1);
        assert_eq!(analytics::fill(&store, 100).await.unwrap(), 1);
        assert_eq!(summary(&store, json!({})).await.failures.checks[0].runs, 1);
        assert_eq!(analytics::fill(&store, 100).await.unwrap(), 0);
    });
}
