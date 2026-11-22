//! What both review-route tests share. AI review's switch is read once per process, so each
//! switch is a test binary of its own (`review_routes_off.rs`, `review_routes_on.rs`), each with
//! one test that sets it first.

use serde_json::json;
use shaked::model::*;

use crate::support::{records, AppFx};

/// Sets AI review to `switch`, reviewing on the idle app's local provider (`local:custom`: it
/// needs no key, and no case reaches it). Call it before the runtime starts.
pub fn review(switch: &str) {
    // SAFETY: set before the runtime or any thread that reads the environment starts.
    unsafe {
        std::env::set_var("SHAKEDOWN_REVIEW", switch);
        std::env::set_var("SHAKEDOWN_REVIEW_MODEL", "local:custom:m");
    }
}

/// An app that runs no agent.
pub async fn app(tag: &str) -> AppFx {
    AppFx::idle("review_routes", tag).start().await
}

/// A new branch with no agent: running, or ended with `status`.
pub async fn branch(fx: &AppFx, status: Option<BranchStatus>) -> String {
    let team = Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![AgentSpec::new("solo", AgentKind::Open).with_model("m")],
    };
    let store = records(&fx.state.store);
    let run_id = store.create_run("27", &team).await.unwrap();
    let id = store
        .create_branch(&run_id, None, &team, &json!([]), None)
        .await
        .unwrap();
    if let Some(s) = status {
        store.set_status(&id, s, None).await.unwrap();
    }
    id
}
