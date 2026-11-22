//! Fault X6 (the site meter reads low) removed at a fork, without a model or the factory's stacks:
//! a toy benchmark's world holds the real `utility` integration beside a toy part, and offers the
//! factory's `meter-low` option (`benchmarks/factory/src/options.rs`) on it. The test drives the run
//! as an external system does.
mod support;

use serde_json::{json, Value};
use support::driven::{events, finish, fork, start, Agent};
use support::packages::{package_root, toy, ToyRoot};
use support::{run, AppFx};

/// The factory's `meter-low` option, set up on the `utility` part as the factory sets it up.
fn options() -> Value {
    let after = json!({"name": "after", "label": "After", "kind": "int", "min": 0, "max": 30, "default": 2});
    let percent = json!({"name": "percent", "label": "Low (%)", "kind": "int", "min": 1, "max": 99, "default": 15});
    let low = json!({"id": "meter-low", "label": "Site meter reads low", "description": "",
        "part": "utility", "default": false, "params": [after, percent],
        "template": {"id": "meter-low", "when": {"after_calls": "{after}", "tool": "utility_meter"},
                     "do": {"kind": "meter_low", "percent": "{percent}"},
                     "text": "The site's main meter starts reading {percent}% low"}});
    let setup = json!({"utility": {"tool": "utility_scenario", "args": {"name": "base"}}});
    json!({"base": ["t1"], "disruptions": [low], "setup": setup})
}

fn world(tag: &str) -> ToyRoot {
    let options = options().to_string();
    let args = [
        "bench",
        "--scored",
        "--part",
        "box=toy-box",
        "--part",
        "utility=utility",
        "--scenario-options",
        options.as_str(),
    ];
    package_root(
        tag,
        &[
            toy("toy-box", "integration", &["part"]),
            toy("toybench", "benchmark", &args),
        ],
    )
}

async fn kw(agent: &Agent) -> Value {
    let m = agent.call("utility_meter", json!({})).await;
    m["structuredContent"]["kw"].clone()
}

#[test]
fn x6_removed_at_a_fork_reads_the_true_meter() {
    run(async {
        let fx = AppFx::idle("meterfork", "x6")
            .root(world("meterfork-x6"))
            .bench("toybench");
        let fx = fx.start().await;
        let x6 = json!({"id": "meter-low", "params": {"after": 0}});
        let body = json!({"task_id": "t1", "scenario": {"disruptions": [x6]}});
        let (branch, solo) = start(&fx, body).await;
        assert_eq!(kw(&solo).await, 1275.0, "15% low");
        finish(&fx, &branch).await;
        let evs = events(&fx, &branch).await;
        let at = evs
            .iter()
            .find(|e| e.payload["name"] == "utility_meter")
            .unwrap()
            .seq;
        let (same, agent) = fork(&fx, &branch, json!({"at_seq": at})).await;
        assert_eq!(kw(&agent).await, 1275.0, "the same schedule as the parent");
        finish(&fx, &same).await;
        let remove = json!({"at_seq": at, "disruptions": {"remove": ["meter-low"]}});
        let (undone, agent) = fork(&fx, &branch, remove).await;
        assert_eq!(kw(&agent).await, 1500.0, "the true load");
        finish(&fx, &undone).await;
    })
}
