//! World model calls: a part (and a benchmark's scoring) calls models through the gateway with the
//! endpoint shaked gave it, and each call is recorded under its world actor, charged and counted.
mod support;
use support::records;

use std::{sync::Arc, time::Duration};

use serde_json::{json, Value};
use shaked::model::*;
use shaked::providers::{Endpoint, LocalEndpoint, ProviderConfig};
use support::{fake_llm, packages, run, AppFx};

/// Providers whose default is the local server `pod` at `base`.
fn pod(base: &str) -> ProviderConfig {
    let mut providers = ProviderConfig::default();
    providers.locals.push(LocalEndpoint {
        name: "pod".into(),
        endpoint: Endpoint::new(base, None),
        rate: None,
    });
    providers.default = "local:pod".into();
    providers
}

/// A chat answer saying `text`, with its usage.
fn says(text: &str) -> Value {
    json!({"choices": [{"message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
           "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})
}

/// The agent pokes once and then says it is done; the customer and the judge answer by model.
fn answerer() -> fake_llm::Answerer {
    Arc::new(|body: &Value| match body["model"].as_str() {
        Some("customer") => says("I want a refund."),
        Some("judge") => says("fair"),
        _ if body["messages"].as_array().unwrap().last().unwrap()["role"] == "tool" => {
            says("Done.")
        }
        _ => json!({"choices": [{"message": {"role": "assistant", "content": "", "tool_calls": [
            {"id": "c1", "type": "function", "function": {"name": "poke", "arguments": "{}"}}]},
            "finish_reason": "tool_calls"}], "usage": {"prompt_tokens": 1, "completion_tokens": 1}}),
    })
}

/// The events of `branch` once it has ended.
async fn ended(fx: &AppFx, branch: &str) -> Vec<Event> {
    for _ in 0..600 {
        let (_, b) = fx.get(&format!("/api/branches/{branch}")).await;
        if !b["ended_at"].is_null() {
            return records(&fx.state.store).events(branch).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the run did not end");
}

/// A run of one API-mode agent on the toy world whose part asks the customer model on `poke`
/// (as `customer`, on the named provider) and whose scoring asks the judge (on the default one).
async fn judged_run(tag: &str) -> (AppFx, Vec<Event>, Value) {
    let llm = fake_llm::fake_llm_with(answerer()).await;
    let part = ["--ask-model", "local:pod", "--ask-actor", "customer"];
    let root = packages::toy_world(tag, &part, &["--scored", "--judge"]);
    let fx = AppFx::idle("worldmodels", tag)
        .root(root)
        .bench("toybench")
        .providers(pod(&llm.base))
        .start()
        .await;
    let agent = AgentSpec::new("solo", AgentKind::Open)
        .with_model("m")
        .with_prompt("Poke it.");
    let team = json!({"id": "t", "name": "T", "agents": [agent]});
    let (status, v) = fx
        .post("/api/runs", json!({"task_id": "t1", "team": team}))
        .await;
    assert_eq!(status, 200, "{v}");
    let branch = v["branch_id"].as_str().unwrap().to_string();
    let evs = ended(&fx, &branch).await;
    let (_, detail) = fx.get(&format!("/api/branches/{branch}")).await;
    (fx, evs, detail)
}

fn calls_of<'a>(evs: &'a [Event], actor: &str) -> Vec<&'a Event> {
    let mine = evs.iter().filter(|e| e.kind == EventKind::ModelCall);
    mine.filter(|e| e.agent.as_deref() == Some(actor)).collect()
}

/// The branch's check holds the scoring's `judged` check, passed.
fn assert_judged_passed(evs: &[Event]) {
    let check = evs.iter().find(|e| e.kind == EventKind::Check).unwrap();
    let judged = check.payload["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "judged");
    assert_eq!(judged.unwrap()["passed"], true, "{}", check.payload);
}

#[test]
fn a_parts_and_the_scorings_model_calls_are_recorded_under_world_actors() {
    run(async {
        let (_fx, evs, _) = judged_run("record").await;
        let customer = calls_of(&evs, "world.box.customer");
        assert_eq!(
            customer.len(),
            1,
            "{:?}",
            evs.iter().map(|e| (&e.agent, e.kind)).collect::<Vec<_>>()
        );
        assert_eq!(customer[0].payload["request"]["model"], "customer");
        assert_eq!(customer[0].payload["provider"], "local:pod");
        let poke = evs.iter().find(|e| e.kind == EventKind::ToolCall).unwrap();
        assert!(
            customer[0].seq < poke.seq,
            "made during the poke, recorded before its step"
        );
        assert_eq!(poke.payload["result"]["said"], "I want a refund.");
        assert_eq!(
            poke.payload["result"]["bad_token"], 401,
            "another token is refused"
        );
        assert_eq!(
            calls_of(&evs, "world.scoring").len(),
            1,
            "the judge is the scoring's"
        );
        assert_judged_passed(&evs);
        let points = evs.iter().filter(|e| e.kind == EventKind::SavePoint);
        assert_eq!(points.count(), 0, "no save point before a world's call");
    })
}

#[test]
fn world_actors_count_as_their_own_use_in_the_branchs_metrics() {
    run(async {
        let (_fx, _evs, detail) = judged_run("metrics").await;
        let agents = detail["metrics"]["agents"].as_array().unwrap();
        let of = |id: &str| {
            agents
                .iter()
                .find(|a| a["agent"] == id)
                .unwrap_or_else(|| panic!("{id}: {agents:?}"))
        };
        let customer = of("world.box.customer");
        assert_eq!(
            (&customer["kind"], &customer["model"]),
            (&json!("world"), &json!("customer"))
        );
        assert_eq!(
            (&customer["tokens"], &customer["model_calls"]),
            (&json!(15), &json!(1))
        );
        assert_eq!(of("world.scoring")["model"], "judge");
        let total: i64 = agents.iter().map(|a| a["tokens"].as_i64().unwrap()).sum();
        assert_eq!(
            detail["metrics"]["tokens"], total,
            "the branch's tokens include the world's"
        );
    })
}
