//! The retail benchmark's scheduled disruptions (its scenario options, faults X1 to X4 of the
//! cs2680 evaluation), without a model: the test drives a run of task 27 with one disruption,
//! calls the world as an agent would, and checks that the disruption fires at the scheduled call
//! and only then, that a world event records it, and what the world's saved state holds.
mod support;

use serde_json::{json, Value};
use shaked::model::*;
use support::driven::{events, finish, start, Agent};
use support::{run, AppFx};

/// Delivered, of user yusuf_rossi_9620; its first item is product 6992792935.
const ORDER: &str = "#W2378156";
const PRODUCT: &str = "6992792935";
/// The product's first variant in stock, by id.
const IN_STOCK: &str = "3104857380";
const USER: &str = "ethan_garcia_1261";

async fn setup(tag: &str) -> AppFx {
    AppFx::idle("retaildis", tag).start().await
}

/// A live run of task 27 with the scenario option `id`: its branch and agent.
async fn with(fx: &AppFx, id: &str) -> (String, Agent) {
    start(fx, json!({"scenario": {"disruptions": [{"id": id}]}})).await
}

async fn order(agent: &Agent) -> Value {
    agent
        .call("get_order_details", json!({"order_id": ORDER}))
        .await
}

/// The `disruption` world events of the branch: (the seq of the step before each, its id).
async fn fired(fx: &AppFx, branch: &str) -> Vec<(i64, Value)> {
    let evs = events(fx, branch).await;
    let mut out = vec![];
    for (i, e) in evs.iter().enumerate() {
        if e.kind == EventKind::WorldEvent && e.payload["kind"] == "disruption" {
            out.push((evs[i - 1].seq, e.payload["data"]["id"].clone()));
        }
    }
    out
}

/// The seqs of the branch's tool calls by `solo`.
async fn calls(fx: &AppFx, branch: &str) -> Vec<i64> {
    let evs = events(fx, branch).await;
    let solo = |e: &&Event| e.kind == EventKind::ToolCall && e.agent.as_deref() == Some("solo");
    evs.iter().filter(solo).map(|e| e.seq).collect()
}

#[test]
fn x1_the_first_order_read_reports_a_stale_status() {
    run(async {
        let fx = setup("x1").await;
        let (branch, solo) = with(&fx, "stale-order-status").await;
        let first = order(&solo).await;
        assert_eq!(first["structuredContent"]["status"], "pending", "{first}");
        let again = order(&solo).await;
        assert_eq!(again["structuredContent"]["status"], "delivered");
        let at = calls(&fx, &branch).await[0];
        assert_eq!(
            fired(&fx, &branch).await,
            [(at, json!("stale-order-status"))]
        );
        finish(&fx, &branch).await;
    })
}

#[test]
fn x2_the_first_order_read_times_out() {
    run(async {
        let fx = setup("x2").await;
        let (branch, solo) = with(&fx, "order-read-timeout").await;
        let first = order(&solo).await;
        assert_eq!(first["isError"], true, "{first}");
        let text = first["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("may not have been processed"), "{text}");
        assert_eq!(order(&solo).await["isError"], false);
        assert_eq!(fired(&fx, &branch).await.len(), 1);
        finish(&fx, &branch).await;
    })
}

#[test]
fn x3_the_first_product_read_reports_a_variant_in_stock_as_out_of_stock() {
    run(async {
        let fx = setup("x3").await;
        let (branch, solo) = with(&fx, "variant-out-of-stock").await;
        let args = json!({"product_id": PRODUCT});
        let available =
            |r: &Value| r["structuredContent"]["variants"][IN_STOCK]["available"].clone();
        let first = solo.call("get_product_details", args.clone()).await;
        assert_eq!(available(&first), false, "{first}");
        let again = solo.call("get_product_details", args).await;
        assert_eq!(available(&again), true);
        assert_eq!(fired(&fx, &branch).await.len(), 1);
        finish(&fx, &branch).await;
    })
}

fn address(street: &str) -> Value {
    json!({"user_id": USER, "address1": street, "address2": "", "city": "New York",
           "state": "NY", "country": "USA", "zip": "10001"})
}

async fn street(agent: &Agent) -> Value {
    let u = agent
        .call("get_user_details", json!({"user_id": USER}))
        .await;
    u["structuredContent"]["address"]["address1"].clone()
}

/// The first write answers as if done and writes nothing: the read after it, the step's record
/// and the world's final save all show the old address; the next write goes through.
#[test]
fn x4_the_first_write_reports_success_and_writes_nothing() {
    run(async {
        let fx = setup("x4").await;
        let (branch, solo) = with(&fx, "lost-write").await;
        let before = street(&solo).await;
        let lost = solo.call("modify_user_address", address("1 Lost Lane"));
        let lost = lost.await;
        assert_eq!(lost["isError"], false, "{lost}");
        let answered = &lost["structuredContent"]["address"]["address1"];
        assert_eq!(answered, "1 Lost Lane");
        assert_eq!(street(&solo).await, before);
        assert_lost_write_recorded(&fx, &branch, &before).await;
        let kept = solo.call("modify_user_address", address("2 Kept Road"));
        assert_eq!(kept.await["isError"], false);
        assert_eq!(street(&solo).await, "2 Kept Road");
        finish(&fx, &branch).await;
        assert_final_street(&fx, &branch, "2 Kept Road").await;
    })
}

/// The lost write's step wrote no key, the disruption fired on it, and its save keeps `before`.
async fn assert_lost_write_recorded(fx: &AppFx, branch: &str, before: &Value) {
    let evs = events(fx, branch).await;
    let write = |e: &&Event| e.payload["name"] == "modify_user_address";
    let step = evs.iter().find(write).unwrap();
    assert_eq!(step.payload["writes"], json!([]), "{}", step.payload);
    assert_eq!(fired(fx, branch).await, [(step.seq, json!("lost-write"))]);
    let saved = step.payload["saves"]["retail"].as_str().unwrap();
    assert_street_in(fx, saved, before).await;
}

/// The user's street in the retail part's save `save` is `want`.
async fn assert_street_in(fx: &AppFx, save: &str, want: &Value) {
    let key = format!("users/{USER}");
    let docs = fx.hub.inspect_save(save, &[&key]).await;
    assert_eq!(&docs[&key]["doc"]["address"]["address1"], want, "{docs}");
}

/// The user's street in the branch's final save of the retail part.
async fn assert_final_street(fx: &AppFx, branch: &str, want: &str) {
    let evs = events(fx, branch).await;
    let check = evs.iter().find(|e| e.kind == EventKind::Check).unwrap();
    let save = check.payload["final_saves"]["retail"].as_str().unwrap();
    assert_street_in(fx, save, &json!(want)).await;
}
