//! shaked's own agent loop (open mode). Without a model: what the gateway refuses and records
//! when no model can answer. `live_`: the loop on a real model (an API provider; see
//! `support/live.rs`), every model and tool call recorded with its fields. What only a model
//! that misbehaves on purpose could show (arguments cut off, output cut off, a refusal) is tested
//! on the loop's own functions in `src/agents/open.rs`.
mod support;
use support::records;
use support::team_of;

use std::sync::Arc;

use serde_json::{json, Value};
use shaked::{
    agents::{open::OpenDriver, AgentRun, Driver},
    app::{AppState, Config},
    exec::BranchCtx,
    model::*,
    providers::ProviderConfig,
};
use support::live::Live;
use support::packages::TestHub;
use support::pg::Cleanup;
use support::{app, live, run};

/// An open agent on the default provider that names `model`.
fn agent(id: &str, model: Option<&str>) -> AgentSpec {
    AgentSpec {
        model: model.map(str::to_string),
        ..AgentSpec::new(id, AgentKind::Open).with_prompt("p")
    }
}

struct Fx {
    state: Arc<AppState>,
    ctx: Arc<BranchCtx>,
    _hub: TestHub,
    _clean: Cleanup,
}

/// A live branch of a retail world for `agents`, on an app with `providers`.
async fn setup(tag: &str, agents: Vec<AgentSpec>, providers: ProviderConfig) -> Fx {
    let hub = TestHub::new(&[]);
    let (store, clean) = app::meta_store("open", tag).await;
    let team = team_of(agents);
    let run_id = records(&store).create_run("task", &team).await.unwrap();
    let branch = records(&store)
        .create_branch(&run_id, None, &team, &json!([]), None)
        .await
        .unwrap();
    let world = hub.start_world("retail", &branch).await;
    let (listener, base) = app::bind().await;
    let mut config = Config::new();
    config.providers = providers;
    config.self_base = base;
    let state = Arc::new(AppState::new(store, hub.hub.clone(), config));
    let ctx = BranchCtx::new(branch.clone(), world, team, "task".into());
    let ctx = Arc::new(support::in_store(ctx, &state.store));
    state.live.insert(ctx.clone());
    app::serve(listener, &state);
    Fx {
        state,
        ctx,
        _hub: hub,
        _clean: clean,
    }
}

/// Providers whose default is the local server [`app::NOWHERE`], where nothing listens.
fn nowhere() -> ProviderConfig {
    ProviderConfig::with_upstream(app::NOWHERE)
}

async fn teardown(fx: Fx) {
    fx.ctx.world.stop().await;
}

fn agent_run(fx: &Fx, idx: usize, input: &str, resume: Option<Vec<Value>>) -> AgentRun {
    AgentRun {
        branch: fx.ctx.clone(),
        spec: fx.ctx.roster.team.agents[idx].clone(),
        input: input.into(),
        resume,
    }
}

async fn events(fx: &Fx) -> Vec<Event> {
    records(&fx.state.store)
        .events(&fx.ctx.branch_id)
        .await
        .unwrap()
}

fn gateway_url(fx: &Fx, branch: &str, agent: &str) -> String {
    format!(
        "{}/gateway/{branch}/{agent}/v1/chat/completions",
        fx.state.config.self_base
    )
}

/// A rate-limited call (429) is waited out until the limit resets (OpenRouter quotes the reset
/// in its error body; here 2 s to wait, where the first retry would otherwise wait 1 s) and tried
/// again: the agent goes on, and both tries are recorded, the first with its 429.
#[test]
fn a_rate_limited_call_waits_for_the_reset_and_is_tried_again() {
    run(async {
        let limited = json!({"error": {"code": 429, "message": "Rate limit exceeded",
            "metadata": {"headers": {"X-RateLimit-Reset": "2"}}}});
        let ok = support::fake_llm::gpt_oss("done", Some("All done."));
        let llm = support::fake_llm::fake_llm(vec![(429, limited), (200, ok)]).await;
        let providers = ProviderConfig::with_upstream(&llm.base);
        let fx = setup("ratelimit", vec![agent("solo", Some("m"))], providers).await;
        let r = OpenDriver::new(fx.state.clone())
            .run(agent_run(&fx, 0, "hello", None))
            .await;
        assert!(r.ok, "{:?}", r.error);
        let evs = events(&fx).await;
        let calls: Vec<&Event> = (evs.iter())
            .filter(|e| e.kind == EventKind::ModelCall)
            .collect();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].payload["response"]["status"], 429);
        let waited = calls[1].at - calls[0].at;
        assert!(waited.num_milliseconds() >= 1_900, "{waited}");
        teardown(fx).await;
    })
}

/// A provider nothing answers on: the call is recorded as failed (no cost, no provider
/// response) and the agent fails as an infrastructure problem, not the model's.
#[test]
fn an_unreachable_provider_is_recorded_and_an_infra_failure() {
    run(async {
        let fx = setup("down", vec![agent("solo", Some("m"))], nowhere()).await;
        let r = OpenDriver::new(fx.state.clone())
            .run(agent_run(&fx, 0, "hello", None))
            .await;
        assert!(!r.ok);
        assert_eq!(r.failure, None, "{:?}", r.error);
        let evs = events(&fx).await;
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, EventKind::ModelCall);
        let mc: ModelCallPayload = serde_json::from_value(evs[0].payload.clone()).unwrap();
        assert!(mc.response["error"].is_string(), "{}", mc.response);
        assert_eq!((mc.cost_usd, mc.cost_source), (None, None));
        assert_eq!(mc.provider.as_deref(), Some("local:custom"));
        teardown(fx).await;
    })
}

/// Only OpenRouter has a default model (`tests/providers.rs` checks it is sent): an agent on
/// another provider that names none fails before any call.
#[test]
fn missing_model_without_a_provider_default_fails_before_calling() {
    run(async {
        let fx = setup("defmodel", vec![agent("solo", None)], nowhere()).await;
        let r = OpenDriver::new(fx.state.clone())
            .run(agent_run(&fx, 0, "hello", None))
            .await;
        assert!(!r.ok);
        let error = r.error.unwrap();
        assert!(
            error.contains("names no model") && error.contains("local:custom"),
            "{error}"
        );
        assert!(events(&fx).await.is_empty(), "no call was made");
        teardown(fx).await;
    })
}

#[test]
fn gateway_refuses_unknown_branch_or_agent_without_calling_a_provider() {
    run(async {
        let fx = setup("gw404", vec![agent("solo", Some("m"))], nowhere()).await;
        let body = json!({"model": "m", "messages": []});
        for (b, a) in [("nobranch", "solo"), (fx.ctx.branch_id.as_str(), "nobody")] {
            let r = reqwest::Client::new()
                .post(gateway_url(&fx, b, a))
                .bearer_auth(fx.ctx.agent_token(a))
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), 404);
            let v: Value = r.json().await.unwrap();
            assert!(v["error"].is_string());
        }
        assert!(events(&fx).await.is_empty(), "nothing was recorded");
        teardown(fx).await;
    })
}

/// An agent given a bearer token (an `http` agent while shaked waits for it) has its model calls
/// refused without that token, and nothing is recorded; with it the call goes on as before.
#[test]
fn gateway_wants_the_agents_bearer_token_once_it_has_one() {
    run(async {
        let fx = setup("gwtoken", vec![agent("solo", Some("m"))], nowhere()).await;
        let token = fx.ctx.roster.issue_token("solo", fx.ctx.token_ttl());
        let body = json!({"model": "m", "messages": [{"role": "user", "content": "x"}]});
        let url = gateway_url(&fx, &fx.ctx.branch_id, "solo");
        for bearer in [None, Some("wrong")] {
            let mut req = reqwest::Client::new().post(&url).json(&body);
            if let Some(b) = bearer {
                req = req.bearer_auth(b);
            }
            let r = req.send().await.unwrap();
            assert_eq!(r.status(), 401, "bearer {bearer:?}");
            let v: Value = r.json().await.unwrap();
            assert_eq!(v["error"], shaked::external::TOKEN_WANTED);
            let why = v["error"].as_str().unwrap_or_default();
            assert!(why.contains("`token`") && why.contains("shakedown-sdk 0.2.0"));
        }
        assert!(events(&fx).await.is_empty(), "nothing was recorded");
        let r = reqwest::Client::new()
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 502, "the call reached the (closed) provider");
        assert_eq!(events(&fx).await.len(), 1);
        teardown(fx).await;
    })
}

/// A provider the configuration doesn't know is recorded as a failed call, with no provider.
#[test]
fn an_unknown_provider_is_recorded_as_a_failed_call() {
    run(async {
        let mut a = agent("solo", Some("m"));
        a.provider = Some("local:gone".into());
        let fx = setup("gone", vec![a], nowhere()).await;
        let body = json!({"model": "m", "messages": [{"role": "user", "content": "x"}]});
        let r = reqwest::Client::new()
            .post(gateway_url(&fx, &fx.ctx.branch_id, "solo"))
            .bearer_auth(fx.ctx.agent_token("solo"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 502);
        let evs = events(&fx).await;
        assert_eq!(evs.len(), 1);
        let mc: ModelCallPayload = serde_json::from_value(evs[0].payload.clone()).unwrap();
        let error = mc.response["error"].as_str().unwrap();
        assert!(error.contains("unknown provider \"local:gone\""), "{error}");
        assert_eq!((mc.provider, mc.cost_usd), (None, None));
        teardown(fx).await;
    })
}

// ---------------------------------------------------------------- on a real model

/// The configured providers (the live test's key among them).
fn env_providers() -> ProviderConfig {
    Config::new().providers
}

/// Each tool call answers a tool call the model call before it chose (same id), and its result
/// went back to the model in the next request, under that id.
fn assert_tool_calls_answer_the_model(evs: &[Event]) {
    let mut chosen: Vec<String> = vec![];
    for (i, e) in evs.iter().enumerate() {
        if e.kind == EventKind::ModelCall {
            let calls = e.payload["response"]["choices"][0]["message"]["tool_calls"].as_array();
            chosen = calls
                .into_iter()
                .flatten()
                .filter_map(|c| c["id"].as_str().map(str::to_string))
                .collect();
            continue;
        }
        if e.kind != EventKind::ToolCall {
            continue;
        }
        let tc: ToolCallPayload = serde_json::from_value(e.payload.clone()).unwrap();
        let id = tc.tool_call_id.expect("an open agent's call names its id");
        assert!(
            chosen.contains(&id),
            "{id} not chosen by the model call before it"
        );
        let next = evs[i..].iter().find(|n| n.kind == EventKind::ModelCall);
        if let Some(n) = next {
            let msgs = n.payload["request"]["messages"].as_array().unwrap();
            let back = msgs
                .iter()
                .any(|m| m["role"] == "tool" && m["tool_call_id"] == id);
            assert!(back, "the result of {id} went back to the model");
        }
    }
}

/// The agent's first request: its system prompt, the input, and the world's tools (no handoff
/// for an agent that is the last of its team).
fn assert_first_request(evs: &[Event], prompt: &str, input: &str) {
    let first = evs.iter().find(|e| e.kind == EventKind::ModelCall).unwrap();
    let req = &first.payload["request"];
    assert_eq!(
        req["messages"][0],
        json!({"role": "system", "content": prompt})
    );
    assert_eq!(
        req["messages"][1],
        json!({"role": "user", "content": input})
    );
    let tools = live::offered_tools(evs, "solo").unwrap();
    assert!(tools.iter().any(|t| t == "get_order_details"), "{tools:?}");
    assert!(!tools.iter().any(|t| t == "handoff"), "{tools:?}");
}

live_test!(
    live_open_agent_records_every_model_and_tool_call,
    open_agent_records_every_model_and_tool_call,
    api
);

/// Every model call is the solo agent's, with its model and provider and its usage.
fn assert_model_calls_named(evs: &[Event], live: &Live) {
    for e in evs.iter().filter(|e| e.kind == EventKind::ModelCall) {
        let mc: ModelCallPayload = serde_json::from_value(e.payload.clone()).unwrap();
        assert_eq!(mc.request["model"], live.model());
        assert_eq!(mc.provider.as_deref(), live.provider());
        assert_eq!(e.agent.as_deref(), Some("solo"));
        let prompt = mc.usage["prompt_tokens"].as_i64().unwrap_or(0);
        assert!(prompt > 0, "{}", mc.usage);
    }
}

/// The loop on a real model: every model call is recorded with the agent's model and provider
/// and its usage, every tool call answers the model call that chose it and goes back to it, and
/// the totals are the sum of the calls.
async fn open_agent_records_every_model_and_tool_call(live: Live) {
    let prompt = "You are a retail support agent. Use the tools to answer.";
    let input = "What is the status of order #W7181492? Look it up with get_order_details.";
    let fx = setup("live", vec![live.agent("solo", prompt)], env_providers()).await;
    let r = OpenDriver::new(fx.state.clone())
        .run(agent_run(&fx, 0, input, None))
        .await;
    assert!(r.ok || r.failure.is_some(), "{:?}", r.error);
    let evs = events(&fx).await;
    assert_model_calls_named(&evs, &live);
    assert_first_request(&evs, prompt, input);
    assert_tool_calls_answer_the_model(&evs);
    let summed: i64 = evs
        .iter()
        .filter(|e| e.kind == EventKind::ModelCall)
        .map(|e| e.payload["usage"]["total_tokens"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(r.tokens.total_tokens, summed);
    teardown(fx).await;
}

live_test!(
    live_resume_replaces_the_system_prompt_and_a_handoff_stops_the_loop,
    resume_replaces_the_system_prompt_and_a_handoff_stops_the_loop,
    api
);

/// When the agent handed off, it ended well right there, its note kept.
fn assert_handoff_stopped_the_loop(fx: &Fx, evs: &[Event], ok: bool) {
    let Some(i) = evs.iter().position(|e| e.kind == EventKind::Handoff) else {
        eprintln!("note: the model did not hand off this time; the handoff part was not exercised");
        return;
    };
    assert!(ok);
    let stopped = evs[i + 1..].iter().all(|e| e.kind != EventKind::ModelCall);
    assert!(stopped, "the loop stopped");
    let note = fx
        .ctx
        .mailbox
        .handoff_note("triage")
        .expect("the note is kept");
    assert_eq!(json!(note), evs[i].payload["note"]);
}

/// The first request carries the new system prompt over the recorded conversation (not the
/// input), as the canonical request (no provider's own fields).
fn assert_resumed_request(evs: &[Event], prompt: &str) {
    let first = evs.iter().find(|e| e.kind == EventKind::ModelCall).unwrap();
    let request = &first.payload["request"];
    let msgs = request["messages"].as_array().unwrap();
    assert_eq!(msgs[0], json!({"role": "system", "content": prompt}));
    let recorded = "The customer wants a new address on her account.";
    assert_eq!(msgs[1]["content"], recorded);
    assert_eq!(msgs.len(), 2, "the recorded conversation, not the input");
    assert_eq!(request.get("usage"), None);
}

/// A resumed agent gets its new system prompt over the recorded conversation, an agent before
/// the last is offered `handoff`, and when it hands off the loop stops there with its note.
async fn resume_replaces_the_system_prompt_and_a_handoff_stops_the_loop(live: Live) {
    let prompt = "You are triage. Do not call any other tool: call handoff at once with a short note for the resolver.";
    let agents = vec![live.agent("triage", prompt), live.agent("resolver", "r")];
    let fx = setup("handoff", agents, env_providers()).await;
    let history = vec![
        json!({"role": "system", "content": "OLD PROMPT"}),
        json!({"role": "user", "content": "The customer wants a new address on her account."}),
    ];
    let r = OpenDriver::new(fx.state.clone())
        .run(agent_run(&fx, 0, "unused", Some(history)))
        .await;
    assert!(r.ok || r.failure.is_some(), "{:?}", r.error);
    let evs = events(&fx).await;
    assert_resumed_request(&evs, prompt);
    let tools = live::offered_tools(&evs, "triage").unwrap();
    assert!(tools.iter().any(|t| t == "handoff"), "{tools:?}");
    assert_handoff_stopped_the_loop(&fx, &evs, r.ok);
    teardown(fx).await;
}
