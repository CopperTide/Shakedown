//! What a fork may change beyond an agent's prompt and model, without a model: the test drives
//! a retail run as an external system does (registers its agent and calls the world through the
//! agent's MCP endpoint), finishes it, forks it and calls the world again on the new branch.
mod support;
use support::records;

use serde_json::{json, Value};
use shaked::model::*;
use support::driven::{events, finish, fork, on, start, Agent};
use support::{run, AppFx};

const ORDER: &str = "#W2378156";

async fn setup(tag: &str) -> AppFx {
    AppFx::idle("forkopts", tag).start().await
}

/// The retail world's tool names.
async fn world_tools(fx: &AppFx) -> Vec<String> {
    let bench = fx.state.hub.bench_describe("retail").await.unwrap();
    bench.tools.into_iter().map(|t| t.name).collect()
}

/// The seq of the first tool call on the branch.
fn first_call(evs: &[Event]) -> i64 {
    let call = evs.iter().find(|e| e.kind == EventKind::ToolCall);
    call.expect("a tool call").seq
}

/// The forked agent is offered only `get_order_details` of the world's tools, may call it, and
/// is refused another.
async fn assert_only_order_details(fx: &AppFx, forked: &Agent) {
    let world = world_tools(fx).await;
    let offered: Vec<String> = (forked.tools().await.into_iter())
        .filter(|t| world.contains(t))
        .collect();
    assert_eq!(offered, ["get_order_details"]);
    let read = forked.call("get_order_details", json!({"order_id": ORDER}));
    let read = read.await;
    assert_eq!(read["isError"], false, "{read}");
    let refused = forked.call("get_user_details", json!({"user_id": "x"}));
    let refused = refused.await;
    assert_eq!(refused["isError"], true, "{refused}");
    let text = refused["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("setup does not list"), "{text}");
}

#[test]
fn a_fork_may_give_an_agent_exactly_the_tools_it_lists() {
    run(async {
        let fx = setup("tools").await;
        let (branch, solo) = start(&fx, json!({})).await;
        let read = solo.call("get_order_details", json!({"order_id": ORDER}));
        assert_eq!(read.await["isError"], false);
        finish(&fx, &branch).await;
        let at = first_call(&events(&fx, &branch).await);
        let only = json!({"solo": {"tools": ["get_order_details"]}});
        let body = json!({"at_seq": at, "changes": only});
        let (child, forked) = fork(&fx, &branch, body).await;
        assert_only_order_details(&fx, &forked).await;
        // The parent keeps every tool; the new branch records its list.
        let parent = records(&fx.state.store).branch(&branch).await.unwrap();
        assert!(parent.team.agents[0].tools.is_empty());
        let stored = records(&fx.state.store).branch(&child).await.unwrap();
        assert_eq!(stored.team.agents[0].tools, ["get_order_details"]);
        finish(&fx, &child).await;
        // A tool the world doesn't have is refused before anything is made.
        let typo = json!({"at_seq": at, "changes": {"solo": {"tools": ["get_order"]}}});
        let (status, v) = on(&fx, &branch, "fork", typo).await;
        assert_eq!(status, 400, "{v}");
        assert!(v["error"].as_str().unwrap().contains("get_order"), "{v}");
    })
}

fn address(street: &str) -> Value {
    json!({"user_id": USER, "address1": street, "address2": "", "city": "New York",
           "state": "NY", "country": "USA", "zip": "10001"})
}

const USER: &str = "ethan_garcia_1261";

/// The street of `USER`'s address as `agent` reads it.
async fn street(agent: &Agent) -> Value {
    let u = agent
        .call("get_user_details", json!({"user_id": USER}))
        .await;
    assert_eq!(u["isError"], false, "{u}");
    u["structuredContent"]["address"]["address1"].clone()
}

/// A finished run whose agent moved `USER`: the branch, the seq of the write and the street the
/// user had before it.
async fn moved_user(fx: &AppFx) -> (String, i64, Value) {
    let (branch, solo) = start(fx, json!({})).await;
    let before = street(&solo).await;
    let w = solo
        .call("modify_user_address", address("7 Fork Road"))
        .await;
    assert_eq!(w["isError"], false, "{w}");
    finish(fx, &branch).await;
    let evs = events(fx, &branch).await;
    let write = evs.iter().rev().find(|e| e.kind == EventKind::ToolCall);
    (branch, write.unwrap().seq, before)
}

#[test]
fn restore_none_leaves_the_world_as_the_parent_ended() {
    run(async {
        let fx = setup("none").await;
        let (branch, at, before) = moved_user(&fx).await;
        assert_ne!(before, "7 Fork Road");
        // The default restores the world as it was just before the write.
        let (exact, agent) = fork(&fx, &branch, json!({"at_seq": at})).await;
        assert_eq!(street(&agent).await, before);
        finish(&fx, &exact).await;
        // `none` keeps the agents' context up to the write but the world as the run ended.
        let none = json!({"at_seq": at, "restore": "none"});
        let (child, agent) = fork(&fx, &branch, none).await;
        assert_eq!(street(&agent).await, "7 Fork Road");
        let b = records(&fx.state.store).branch(&child).await.unwrap();
        assert_eq!(json!(b.restore_mode), "none");
        assert_eq!(
            json!(b.restored),
            json!([{"part": "retail", "how": "none"}])
        );
        finish(&fx, &child).await;
        // Its own steps can't be forked: its world at its start isn't recorded.
        let last = events(&fx, &child).await.last().unwrap().seq;
        let (status, v) = on(&fx, &child, "fork", json!({"at_seq": last})).await;
        assert_eq!(status, 409, "{v}");
    })
}
