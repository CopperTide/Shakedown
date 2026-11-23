//! A run begun in two halves, `record_start` then `stand_up`, records what `begin_run` records.
mod support;
use support::records;

use std::sync::Arc;

use serde_json::Value;
use shaked::{
    auth::Caller,
    exec::{BranchCtx, StoreRecords},
    launch::LaunchSpec,
    model::*,
    runner::{self, BranchPlan, RunError},
    scenario::Scenario,
};
use support::{run, AppFx};

fn team() -> Team {
    Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![AgentSpec::new("a", AgentKind::ClaudeCode).with_model("sonnet")],
    }
}

/// Each event's kind and payload, without what changes from run to run (its ids, times and
/// overhead).
async fn shape(fx: &AppFx, branch: &str) -> Vec<(EventKind, Value)> {
    let evs = records(&fx.state.store).events(branch).await.unwrap();
    let mut shape: Vec<_> = evs.into_iter().map(|e| (e.kind, e.payload)).collect();
    for (_, payload) in &mut shape {
        if let Some(o) = payload.as_object_mut() {
            o.remove("overhead_ms");
            o.remove("world_epoch_ms");
        }
    }
    shape
}

fn spec<'a>(scope: &Arc<Caller>, task: &'a str, scenario: &'a Scenario) -> LaunchSpec<'a> {
    LaunchSpec {
        scope: scope.clone(),
        task_id: task,
        team: team(),
        scenario,
        rules: Rules::new(Runner::Chain),
        driver: None,
        benchmark: None,
    }
}

/// The run begun in two halves: its plan and its stood-up branch.
async fn in_halves(
    fx: &AppFx,
    scope: &Arc<Caller>,
    scenario: &Scenario,
) -> (BranchPlan, BranchCtx) {
    let spec = spec(scope, "27", scenario);
    let plan = runner::record_start(&fx.state, &spec).await.unwrap();
    let sink = StoreRecords::new(
        fx.state.store.clone(),
        scope.clone(),
        plan.branch_id.clone(),
    );
    let ctx = runner::stand_up(&fx.state, &plan, Arc::new(sink))
        .await
        .unwrap();
    (plan, ctx)
}

#[test]
fn halves_record_the_same_events_as_begin_run() {
    run(async {
        let fx = AppFx::configured("halves", "same")
            .bench("retail")
            .start()
            .await;
        let scope = Arc::new(Caller::person());
        let scenario = Scenario {
            seed: Some(7),
            ..Default::default()
        };
        let whole = spec(&scope, "27", &scenario);
        let (_, whole) = runner::begin_run(&fx.state, &whole).await.unwrap();
        let (plan, ctx) = in_halves(&fx, &scope, &scenario).await;
        assert_eq!((plan.benchmark.as_str(), plan.clock_from_ms), ("retail", 0));
        let json = serde_json::to_string(&plan).unwrap();
        let back: BranchPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.run_id, plan.run_id);
        let (a, b) = (
            shape(&fx, &whole.branch_id).await,
            shape(&fx, &ctx.branch_id).await,
        );
        assert_eq!(a.first().map(|e| e.0), Some(EventKind::RunStart));
        assert_eq!(a, b);
        whole.world.stop().await;
        ctx.world.stop().await;
    });
}

#[test]
fn record_start_refuses_an_unknown_task() {
    run(async {
        let fx = AppFx::configured("halves", "unknown")
            .bench("retail")
            .start()
            .await;
        let scope = Arc::new(Caller::person());
        let scenario = Scenario::default();
        let spec = spec(&scope, "nope", &scenario);
        let err = runner::record_start(&fx.state, &spec).await;
        assert!(matches!(err, Err(RunError::UnknownTask(t)) if t == "nope"));
    });
}
