//! The scenario tool arms disruptions in the retail database; an agent's call counts and fires
//! them and names their internal keys, so the step is saved with the schedule.
mod common;
use common::*;
use serde_json::{json, Value};
use shakedown_part::proto::{BenchCallTool, BenchToolOutput};
use shakedown_part::Benchmark;

async fn call(url: &str, name: &str, args: Value) -> BenchToolOutput {
    let p = BenchCallTool {
        name: name.into(),
        args,
        connections: conns(url),
    };
    retail().call_tool(p).await.unwrap()
}

fn timeout_after(n: u64) -> Value {
    json!({"id": "slow", "when": {"after_calls": n, "tool": "get_order_details"},
           "do": {"kind": "timeout"}, "text": "Slow"})
}

#[tokio::test]
async fn an_armed_disruption_is_counted_and_fires_once_on_its_call() {
    let db = TestDb::seeded("disrupt").await;
    let armed = call(
        &db.url,
        "retail_scenario",
        json!({"disruptions": [timeout_after(1)]}),
    )
    .await;
    assert_eq!(
        (armed.error, armed.writes),
        (None, vec!["_disruption/slow".to_string()])
    );
    let read = json!({"order_id": "#W2378156"});
    let first = call(&db.url, "get_order_details", read.clone()).await;
    assert!(first.error.is_none() && first.events.is_empty());
    assert!(
        first.writes.contains(&"_disruption/slow".to_string()),
        "the count is saved"
    );
    let second = call(&db.url, "get_order_details", read.clone()).await;
    assert!(second
        .error
        .unwrap()
        .contains("may not have been processed"));
    assert_eq!(second.events[0].data["id"], "slow");
    let third = call(&db.url, "get_order_details", read).await;
    assert!(third.error.is_none() && third.events.is_empty() && third.writes.is_empty());
}

#[tokio::test]
async fn a_bad_or_taken_disruption_is_refused_and_arms_nothing() {
    let db = TestDb::seeded("disrupt_bad").await;
    let both = json!({"disruptions": [timeout_after(0), timeout_after(1)]});
    let twice = call(&db.url, "retail_scenario", both).await;
    assert_eq!(
        twice.error.as_deref(),
        Some("two disruptions have the same id")
    );
    let bad = json!({"disruptions": [{"id": "x", "text": "t", "when": {"after_calls": 0}, "do": {"kind": "boom"}}]});
    assert!(call(&db.url, "retail_scenario", bad)
        .await
        .error
        .unwrap()
        .contains("do.kind"));
    let read = call(
        &db.url,
        "get_order_details",
        json!({"order_id": "#W2378156"}),
    )
    .await;
    assert!(
        read.error.is_none() && read.writes.is_empty(),
        "nothing was armed"
    );
}

#[tokio::test]
async fn a_removed_disruption_no_longer_fires() {
    let db = TestDb::seeded("disrupt_rm").await;
    call(
        &db.url,
        "retail_scenario",
        json!({"disruptions": [timeout_after(0)]}),
    )
    .await;
    let removed = call(
        &db.url,
        "retail_scenario",
        json!({"disruptions": [], "remove": ["slow"]}),
    )
    .await;
    assert_eq!(
        (removed.error, removed.writes),
        (None, vec!["_disruption/slow".to_string()])
    );
    let read = call(
        &db.url,
        "get_order_details",
        json!({"order_id": "#W2378156"}),
    )
    .await;
    assert!(read.error.is_none() && read.events.is_empty());
    // An id freed by a removal may be armed again.
    let again = json!({"disruptions": [timeout_after(0)], "remove": []});
    assert_eq!(call(&db.url, "retail_scenario", again).await.error, None);
}
