//! A fork of a `dispatch` run mid-run, without a model: a scripted server stands in for the model
//! of a lead and a worker (open agents), turn by turn from what each was asked. The lead delegates
//! a lookup, the worker reads the order and reports, the lead finishes; a fork at the delegation
//! restarts both there and runs on to the end, on the world as it was at the fork point.
mod support;
use support::records;

use std::sync::Arc;

use serde_json::{json, Value};
use shaked::{
    model::*,
    providers::{Endpoint, LocalEndpoint, ProviderConfig},
};
use support::fake_llm::{fake_llm_with, FakeLlm};
use support::{live, run, AppFx};

const ORDER: &str = "#W2378156";

/// An assistant answer that calls `name` with `args` (its call id names the tool).
fn calls(name: &str, args: Value) -> Value {
    let call = json!({"id": format!("call-{name}"), "type": "function",
                      "function": {"name": name, "arguments": args.to_string()}});
    answer(
        json!({"role": "assistant", "content": null, "tool_calls": [call]}),
        "tool_calls",
    )
}

fn says(text: &str) -> Value {
    answer(json!({"role": "assistant", "content": text}), "stop")
}

fn answer(message: Value, finish: &str) -> Value {
    json!({"id": "c", "object": "chat.completion", "model": "m",
           "choices": [{"index": 0, "finish_reason": finish, "message": message}],
           "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})
}

/// The tool whose result the request ends with, if it ends with one.
fn answered_tool(msgs: &[Value]) -> Option<String> {
    let last = msgs.last()?;
    let id = last["tool_call_id"]
        .as_str()
        .filter(|_| last["role"] == "tool")?;
    id.strip_prefix("call-").map(str::to_string)
}

/// The lead: delegates the lookup, waits, finishes on the worker's report.
fn lead(msgs: &[Value]) -> Value {
    let last = msgs
        .last()
        .map_or("", |m| m["content"].as_str().unwrap_or(""));
    match answered_tool(msgs).as_deref() {
        Some("finish") => says("Done."),
        Some(_) => says("Waiting for the worker."),
        None if last.contains("# Report from") => {
            calls("finish", json!({"summary": "The worker read the order."}))
        }
        None => calls(
            "delegate",
            json!({"to": "worker", "task": format!("Read order {ORDER}.")}),
        ),
    }
}

/// The worker: reads the order, reports what it found, stops.
fn worker(msgs: &[Value]) -> Value {
    match answered_tool(msgs).as_deref() {
        Some("get_order_details") => calls("report", json!({"text": "The order is delivered."})),
        Some(_) => says("Reported."),
        None => calls("get_order_details", json!({"order_id": ORDER})),
    }
}

/// Answers as the lead when the request offers `delegate`, else as the worker.
fn script(body: &Value) -> Value {
    let msgs = body["messages"].as_array().cloned().unwrap_or_default();
    let offers = |name: &str| {
        let tools = body["tools"].as_array().into_iter().flatten();
        tools.into_iter().any(|t| t["function"]["name"] == name)
    };
    match offers("delegate") {
        true => lead(&msgs),
        false => worker(&msgs),
    }
}

fn pod(base: &str) -> ProviderConfig {
    let mut c = ProviderConfig::default();
    c.locals.push(LocalEndpoint {
        name: "pod".into(),
        endpoint: Endpoint::new(base, None),
        rate: None,
    });
    c.default = "local:pod".into();
    c
}

fn agent(id: &str, lead: bool) -> Value {
    json!({"id": id, "name": id, "kind": "open", "provider": "local:pod", "model": "m",
           "prompt": format!("You are the {id}."), "lead": lead})
}

/// A finished dispatch run of task 27 on the scripted model, with the run request's `extra`
/// fields: the app and the branch.
async fn dispatched(tag: &str, llm: &FakeLlm, extra: Value) -> (AppFx, String) {
    let fx = AppFx::idle("dispatchfork", tag)
        .providers(pod(&llm.base))
        .start()
        .await;
    let team =
        json!({"id": "t", "name": "T", "agents": [agent("lead", true), agent("worker", false)]});
    let mut body = json!({"task_id": "27", "team": team, "runner": "dispatch"});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let (status, v) = fx.post("/api/runs", body).await;
    assert_eq!(status, 200, "{v}");
    let branch = v["branch_id"].as_str().unwrap().to_string();
    let b = live::ended(&fx.state, &branch).await;
    assert_ne!(b.status, BranchStatus::InfraError, "{:?}", b.error);
    (fx, branch)
}

fn of_kind(evs: &[Event], kind: EventKind) -> Vec<&Event> {
    evs.iter().filter(|e| e.kind == kind).collect()
}

/// The order's status in the worker's reads on the branch.
fn read_statuses(evs: &[Event]) -> Vec<Value> {
    let reads = evs
        .iter()
        .filter(|e| e.payload["name"] == "get_order_details");
    reads
        .map(|e| e.payload["result"]["status"].clone())
        .collect()
}

/// On the fork (its events `new`, from the fork point on): the lead is woken to resume, the
/// worker reads again, the lead finishes, both agents end and the run is scored.
fn assert_ran_again(new: &[Event]) {
    let lead = |e: &&Event| e.kind == EventKind::Wake && e.agent.as_deref() == Some("lead");
    let wake = new
        .iter()
        .find(lead)
        .expect("the lead was woken on the fork");
    assert_eq!(wake.payload["reason"], "resume");
    assert_eq!(
        read_statuses(new),
        [json!("delivered")],
        "the worker read again"
    );
    assert_eq!(
        of_kind(new, EventKind::Finish).len(),
        1,
        "the lead finished the fork"
    );
    assert_eq!(
        of_kind(new, EventKind::AgentEnd).len(),
        2,
        "both agents ended"
    );
    assert_eq!(new.last().unwrap().kind, EventKind::Check);
}

#[test]
fn a_fork_of_a_dispatch_run_restarts_its_agents_at_the_delegation_and_runs_to_the_end() {
    run(async {
        let llm = fake_llm_with(Arc::new(script)).await;
        let (fx, branch) = dispatched("mid", &llm, json!({})).await;
        let evs = records(&fx.state.store).events(&branch).await.unwrap();
        assert_eq!(read_statuses(&evs), [json!("delivered")]);
        assert_eq!(
            of_kind(&evs, EventKind::Finish).len(),
            1,
            "the lead finished"
        );
        let at = of_kind(&evs, EventKind::Delegation)[0].seq;
        let new = forked(&fx, &branch, json!({"at_seq": at})).await;
        assert_eq!(new[0].seq, at, "a dispatch run forks where asked");
        assert_ran_again(&new);
    })
}

/// Forks `branch` through the API as `body` asks and waits for the new branch to end: its events
/// from the fork point on.
async fn forked(fx: &AppFx, branch: &str, body: Value) -> Vec<Event> {
    let (status, f) = fx.post(&format!("/api/branches/{branch}/fork"), body).await;
    assert_eq!(status, 200, "{f}");
    let id = f["branch_id"].as_str().unwrap();
    let b = live::ended(&fx.state, id).await;
    assert_ne!(b.status, BranchStatus::InfraError, "{:?}", b.error);
    let at = f["start_seq"].as_i64().unwrap();
    let evs = records(&fx.state.store).events(id).await.unwrap();
    evs.into_iter().filter(|e| e.seq >= at).collect()
}

/// Fork-to-confirm on a dispatch run: the worker met fault X1 (a stale status); a fork at the
/// delegation meets it again, and one that removes X1 there reads the true status.
#[test]
fn a_dispatch_fork_may_remove_the_fault_its_worker_met() {
    run(async {
        let llm = fake_llm_with(Arc::new(script)).await;
        let x1 = json!({"scenario": {"disruptions": [{"id": "stale-order-status"}]}});
        let (fx, branch) = dispatched("unplant", &llm, x1).await;
        let evs = records(&fx.state.store).events(&branch).await.unwrap();
        assert_eq!(read_statuses(&evs), [json!("pending")], "the stale status");
        let at = of_kind(&evs, EventKind::Delegation)[0].seq;
        let again = forked(&fx, &branch, json!({"at_seq": at})).await;
        assert_eq!(read_statuses(&again), [json!("pending")]);
        let remove = json!({"remove": ["stale-order-status"]});
        let undone = forked(&fx, &branch, json!({"at_seq": at, "disruptions": remove})).await;
        assert_eq!(read_statuses(&undone), [json!("delivered")]);
        assert_eq!(of_kind(&undone, EventKind::Finish).len(), 1);
    })
}

/// A fork at the worker's read, while the lead waits: the worker carries on at once and reads
/// again, the lead finishes on its report.
#[test]
fn a_dispatch_fork_at_a_workers_step_carries_the_worker_on() {
    run(async {
        let llm = fake_llm_with(Arc::new(script)).await;
        let (fx, branch) = dispatched("step", &llm, json!({})).await;
        let evs = records(&fx.state.store).events(&branch).await.unwrap();
        let read = evs
            .iter()
            .find(|e| e.payload["name"] == "get_order_details");
        let at = read.unwrap().seq;
        let new = forked(&fx, &branch, json!({"at_seq": at})).await;
        assert_eq!(
            read_statuses(&new),
            [json!("delivered")],
            "the worker read again"
        );
        let reports = of_kind(&new, EventKind::Report).len();
        assert_eq!((reports, of_kind(&new, EventKind::Finish).len()), (1, 1));
    })
}
