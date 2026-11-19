//! How each way of starting a run behaves when the run can't start, and what it records, where
//! the paths differ: a built-in suite records the failure on the slot and goes on, an external
//! suite's claim records it and answers the error, and an external suite's runs take no spend
//! limit. Without a model: the runs never get to an agent.
mod support;

use std::time::Duration;

use arenad::{model::*, store::NewSuite, suite};
use serde_json::json;
use support::{run, AppFx};

async fn setup(tag: &str) -> AppFx {
    AppFx::idle("launch", tag).start().await
}

/// A suite stored as given (the API would refuse an unknown task): `repeats` runs of `task`.
async fn stored_suite(
    fx: &AppFx,
    runner: Runner,
    (task, repeats): (&str, i32),
    max_cost_usd: Option<f64>,
) -> suite::Suite {
    let team = Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![AgentSpec::new("a", AgentKind::ClaudeCode).with_model("sonnet")],
    };
    let (tasks, scenario) = (vec![task.to_string()], Default::default());
    let new = NewSuite {
        name: "s",
        team: &team,
        tasks: &tasks,
        repeats,
        runner,
        benchmark: &fx.state.benchmark,
        scenario: &scenario,
        variants: &[],
        max_cost_usd,
        connection: None,
    };
    fx.state.store.create_suite(new).await.unwrap()
}

/// Each slot's error, in order.
async fn slot_errors(fx: &AppFx, suite: &str) -> Vec<Option<String>> {
    let slots = fx.state.store.slots(suite).await.unwrap();
    slots.into_iter().map(|s| s.error).collect()
}

/// A built-in suite whose run can't start records why on the slot and goes on to the next one:
/// the suite ends without an error of its own.
#[test]
fn a_builtin_suite_goes_on_past_a_run_that_cannot_start() {
    run(async {
        let fx = setup("builtin").await;
        let s = stored_suite(&fx, Runner::Chain, ("nope", 2), None).await;
        suite::spawn_builtin(&fx.state, s.clone());
        let mut ended = None;
        for _ in 0..100 {
            let now = fx.state.store.suite(&s.id).await.unwrap().unwrap();
            if now.ended_at.is_some() {
                ended = Some(now);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let ended = ended.expect("the suite ends");
        assert_eq!(ended.error, None, "the suite itself did not fail");
        let why = Some("unknown task nope".to_string());
        assert_eq!(slot_errors(&fx, &s.id).await, [why.clone(), why]);
    })
}

/// An external suite's claim whose run can't start records why on the slot and answers the
/// error; the next claim finds every slot claimed.
#[test]
fn an_external_claim_answers_the_error_it_records() {
    run(async {
        let fx = setup("claim").await;
        let s = stored_suite(&fx, Runner::External, ("nope", 1), None).await;
        let path = format!("/api/suites/{}/claim", s.id);
        let (status, v) = fx.post(&path, json!({})).await;
        assert_eq!((status, v), (404, json!({"error": "unknown task nope"})));
        let why = Some("unknown task nope".to_string());
        assert_eq!(slot_errors(&fx, &s.id).await, [why]);
        assert_eq!(
            fx.post(&path, json!({})).await,
            (200, json!({"done": true}))
        );
    })
}

/// An external suite's runs start without the suite's spend limit.
#[test]
fn an_external_suites_runs_take_no_spend_limit() {
    run(async {
        let fx = setup("cost").await;
        let s = stored_suite(&fx, Runner::External, ("27", 1), Some(0.5)).await;
        let (status, v) = fx
            .post(&format!("/api/suites/{}/claim", s.id), json!({}))
            .await;
        assert_eq!(status, 200, "{v}");
        let branch = v["branch_id"].as_str().unwrap();
        let evs = fx.state.store.events(branch).await.unwrap();
        assert_eq!(evs[0].kind, EventKind::RunStart);
        assert_ne!(
            evs[0].payload["max_cost_usd"],
            json!(0.5),
            "{}",
            evs[0].payload
        );
        let finish = format!("/api/branches/{branch}/finish");
        assert_eq!(fx.post(&finish, json!({})).await.0, 200);
    })
}
