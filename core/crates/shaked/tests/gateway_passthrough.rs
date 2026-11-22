//! The gateway's pass-through routes for CLI agents: Anthropic Messages and OpenAI Responses go
//! to the provider as the agent sent them, with the org's stored key (never the caller's token),
//! and each call is recorded once, raw, on the control box. The caller here is a sandbox's
//! worker with its worker token; a scripted upstream stands in for the providers. The control
//! box, not the sandbox, keeps such a run to its spend limit and to a pause.
mod support;
use support::{app, records, run, AppFx};

use std::{sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use shaked::{
    app::{AppState, Config},
    auth::{run_tokens::resolve_run_token, Caller},
    jobs::EndIf,
    launch::LaunchSpec,
    model::*,
    runner::{self, BranchPlan},
    scenario::Scenario,
    store::Linked,
    worker::{
        frames::{ToControl, ToWorker, REFUSED_ENDED},
        holds, Ticket,
    },
};
use support::fake_apis::{
    chat_answer, claude_message, fake_apis, store_org_keys, test_vault, FakeApis, Seen, COMPLETED,
    CREATED, MESSAGE_REST, MESSAGE_START, ORG_CLAUDE_KEY, ORG_ROUTER_KEY,
};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A control box whose Anthropic and OpenAI endpoints are `up` (no keys in its environment),
/// with the org's keys stored for both.
async fn control(tag: &str, up: &FakeApis) -> AppFx {
    control_with(tag, up, shaked::app::DEFAULT_LINK_END_GRACE).await
}

/// [`control`], giving a worker told to stop `grace` to report its end.
async fn control_with(tag: &str, up: &FakeApis, grace: Duration) -> AppFx {
    let fx = AppFx::idle("passthrough", tag).bench("retail");
    let fx = fx
        .providers(up.providers())
        .config(move |c| c.link_end_grace = grace);
    let fx = fx.start().await;
    store_org_keys(&fx).await;
    fx
}

/// The control box restarted: a new one on the same store (no links in memory, no timers),
/// giving a worker told to stop `grace`, served at its base.
async fn restarted(fx: &AppFx, up: &FakeApis, grace: Duration) -> (Arc<AppState>, String) {
    let (listener, base) = app::bind().await;
    let mut config = Config::with_providers(up.providers());
    config.self_base = base.clone();
    config.link_end_grace = grace;
    let state = AppState::new(fx.state.store.clone(), fx.state.hub.clone(), config);
    let state = Arc::new(state);
    let _ = state.vault.set(test_vault());
    app::serve(listener, &state);
    (state, base)
}

/// A worker rejoining the control box at `base` with its worker `token`, its last event `n`.
async fn rejoined(base: &str, token: &str, n: u64) -> Ws {
    let url = format!("{}/worker", base.replace("http://", "ws://"));
    let mut ws = tokio_tungstenite::connect_async(url).await.unwrap().0;
    let hello = ToControl::Hello {
        ticket: token.into(),
        resume_from: n,
    };
    say(&mut ws, &hello).await;
    ws
}

fn team() -> Team {
    let agents = vec![
        AgentSpec::new("a", AgentKind::ClaudeCode).with_model("claude-sonnet-5-5"),
        AgentSpec::new("c", AgentKind::Codex)
            .with_provider("openrouter")
            .with_model("gpt-x"),
        AgentSpec::new("o", AgentKind::Open)
            .with_provider("openrouter")
            .with_model("gpt-x"),
        // A provider no key of the caller names.
        AgentSpec::new("k", AgentKind::ClaudeCode)
            .with_provider("key:nope")
            .with_model("claude-sonnet-5-5"),
    ];
    Team {
        id: "t".into(),
        name: "T".into(),
        agents,
    }
}

async fn say(ws: &mut Ws, f: &ToControl) {
    let text = serde_json::to_string(f).unwrap();
    ws.send(Message::text(text)).await.unwrap();
}

/// A branch made by `record_start` for the team under `rules`, and a ticket for its worker.
async fn ticketed(fx: &AppFx, rules: Rules) -> (BranchPlan, String) {
    let scope = Arc::new(Caller::person());
    let scenario = Scenario::default();
    let spec = LaunchSpec {
        scope: scope.clone(),
        task_id: "27",
        team: team(),
        scenario: &scenario,
        rules,
        driver: None,
        benchmark: None,
    };
    let plan = runner::record_start(&fx.state, &scope, &spec).await;
    let plan = plan.unwrap();
    fx.state.links.expect(plan.clone());
    let ticket = Ticket {
        branch_id: plan.branch_id.clone(),
        sandbox: Uuid::new_v4(),
        caller: scope,
    };
    (plan, fx.state.links.tickets.issue(ticket))
}

/// A branch run by a worker: its plan, the worker token its link's `Start` carried, and the
/// link (kept open by the caller).
async fn worker_branch(fx: &AppFx) -> (BranchPlan, String, Ws) {
    worker_branch_under(fx, Rules::new(Runner::Chain)).await
}

/// [`worker_branch`] under `rules`.
async fn worker_branch_under(fx: &AppFx, rules: Rules) -> (BranchPlan, String, Ws) {
    let (plan, ticket) = ticketed(fx, rules).await;
    let (token, ws) = started(fx, &ticket).await;
    (plan, token, ws)
}

/// The next frame the control box sends on the link, pings aside (within 10 s).
async fn next_frame(ws: &mut Ws) -> ToWorker {
    loop {
        let next = tokio::time::timeout(Duration::from_secs(10), ws.next()).await;
        if let Some(Ok(Message::Text(t))) = next.expect("a frame within 10 s") {
            return serde_json::from_str(&t).unwrap();
        }
    }
}

/// Says hello with `ticket`: the worker token of the `Start` it got, and the link.
async fn started(fx: &AppFx, ticket: &str) -> (String, Ws) {
    let url = format!("{}/worker", fx.base.replace("http://", "ws://"));
    let mut ws = tokio_tungstenite::connect_async(url).await.unwrap().0;
    let hello = ToControl::Hello {
        ticket: ticket.into(),
        resume_from: 0,
    };
    say(&mut ws, &hello).await;
    match next_frame(&mut ws).await {
        ToWorker::Start { token, .. } => (token, ws),
        other => panic!("start, not {other:?}"),
    }
}

/// A call of `body` to `api` on `agent`'s gateway route, with `token` as the bearer.
async fn call(
    fx: &AppFx,
    route: (&str, &str, &str),
    token: &str,
    body: &Value,
) -> reqwest::Response {
    call_on(&fx.base, route, token, body).await
}

/// [`call`] on the control box at `base`.
async fn call_on(
    base: &str,
    route: (&str, &str, &str),
    token: &str,
    body: &Value,
) -> reqwest::Response {
    let (branch, agent, api) = route;
    let url = format!("{base}/gateway/{branch}/{agent}/v1/{api}");
    let req = reqwest::Client::new().post(url).bearer_auth(token);
    let req = req.header("anthropic-version", "2023-06-01").json(body);
    req.send().await.unwrap()
}

async fn model_calls(fx: &AppFx, branch: &str) -> Vec<Event> {
    let evs = records(&fx.state.store).events(branch).await.unwrap();
    let calls = evs.into_iter().filter(|e| e.kind == EventKind::ModelCall);
    calls.collect()
}

/// The branch's model calls once there are `n` (within 10 s), and still `n` a moment later.
async fn exactly(fx: &AppFx, branch: &str, n: usize) -> Vec<Event> {
    for _ in 0..100 {
        if model_calls(fx, branch).await.len() >= n {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let calls = model_calls(fx, branch).await;
    assert_eq!(calls.len(), n, "one model_call per call");
    calls
}

/// No header the upstream got carries `token`.
fn never_carries(seen: &Seen, token: &str) {
    for (name, v) in &seen.headers {
        let v = v.to_str().unwrap_or_default();
        assert!(!v.contains(token), "{name} carries the caller's token");
    }
}

/// What the upstream got for the Messages call `ask` made with the worker token `token`.
fn upstream_got_the_orgs_key(seen: &[Seen], ask: &Value, token: &str) {
    assert_eq!(seen.len(), 1);
    let (path, h) = (&seen[0].path, &seen[0].headers);
    assert_eq!((path.as_str(), &seen[0].body), ("/v1/messages", ask));
    assert_eq!(h["x-api-key"], ORG_CLAUDE_KEY, "the org's stored key");
    assert!(h.get("authorization").is_none());
    assert_eq!(h["anthropic-version"], "2023-06-01", "the agent's own");
    never_carries(&seen[0], token);
}

#[test]
fn messages_pass_through_with_the_orgs_key_and_are_recorded() {
    run(async {
        let up = fake_apis().await;
        let fx = control("messages", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let ask = json!({"model": "claude-sonnet-5-5", "max_tokens": 64,
                         "messages": [{"role": "user", "content": "hi"}]});
        let r = call(&fx, (&plan.branch_id, "a", "messages"), &token, &ask).await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.json::<Value>().await.unwrap(), claude_message());
        upstream_got_the_orgs_key(&up.seen(), &ask, &token);
        let calls = exactly(&fx, &plan.branch_id, 1).await;
        let (p, agent) = (&calls[0].payload, calls[0].agent.as_deref());
        assert_eq!(
            (&p["request"], &p["response"], agent),
            (&ask, &claude_message(), Some("a"))
        );
        let u = &p["usage"];
        assert_eq!(
            (&u["prompt_tokens"], &u["completion_tokens"]),
            (&json!(42), &json!(5))
        );
        assert_eq!(
            (&u["input_tokens"], &u["cache_read_input_tokens"]),
            (&json!(12), &json!(30))
        );
        assert_eq!(
            (&p["provider"], &p["api"]),
            (&json!("anthropic"), &json!("messages"))
        );
    })
}

/// The upstream got the org's OpenRouter key as the bearer, and no other key.
fn bears_the_orgs_router_key(seen: &Seen) {
    let auth = seen.headers["authorization"].to_str().unwrap();
    assert_eq!(auth, format!("Bearer {ORG_ROUTER_KEY}"));
    assert!(seen.headers.get("x-api-key").is_none());
}

/// The rest of the stream `r`.
async fn rest_of(r: &mut reqwest::Response) -> String {
    let mut rest = String::new();
    while let Some(chunk) = r.chunk().await.unwrap() {
        rest.push_str(&String::from_utf8_lossy(&chunk));
    }
    rest
}

/// A streamed Responses call's record: the completed response and its usage.
fn recorded_whole(p: &Value) {
    let marks = (&p["api"], &p["stream"]);
    assert_eq!(marks, (&json!("responses"), &json!(true)));
    assert_eq!(p["response"]["id"], "resp_1", "the completed response");
    let u = &p["usage"];
    let tokens = (&u["prompt_tokens"], &u["completion_tokens"]);
    assert_eq!(tokens, (&json!(20), &json!(7)));
}

#[test]
fn responses_pass_through_streaming() {
    run(async {
        let up = fake_apis().await;
        let fx = control("responses", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let ask = json!({"model": "gpt-x", "stream": true, "input": "hi"});
        let mut r = call(&fx, (&plan.branch_id, "c", "responses"), &token, &ask).await;
        assert_eq!(r.status(), 200);
        let kind = r.headers()["content-type"].to_str().unwrap().to_string();
        assert!(kind.starts_with("text/event-stream"), "{kind}");
        let first = tokio::time::timeout(Duration::from_secs(10), r.chunk()).await;
        let first = first.expect("the first event comes while the upstream waits");
        let first = String::from_utf8(first.unwrap().unwrap().to_vec()).unwrap();
        assert!(first.contains("response.created"), "{first}");
        assert!(model_calls(&fx, &plan.branch_id).await.is_empty());
        up.gate.add_permits(1);
        let whole = format!("{first}{}", rest_of(&mut r).await);
        assert_eq!(
            whole,
            format!("{CREATED}{COMPLETED}"),
            "passed on as it came"
        );
        let seen = up.seen();
        bears_the_orgs_router_key(&seen[0]);
        never_carries(&seen[0], &token);
        let calls = exactly(&fx, &plan.branch_id, 1).await;
        recorded_whole(&calls[0].payload);
    })
}

#[test]
fn a_worker_token_reaches_only_its_branch() {
    run(async {
        let up = fake_apis().await;
        let fx = control("own", &up).await;
        let (a, _, _link_a) = worker_branch(&fx).await;
        let (_, b_token, _link_b) = worker_branch(&fx).await;
        let ask = json!({"model": "claude-sonnet-5-5", "max_tokens": 8, "messages": []});
        let at_a = |api| (a.branch_id.as_str(), "a", api);
        for api in ["messages", "responses", "chat/completions"] {
            let r = call(&fx, at_a(api), &b_token, &ask).await;
            assert_eq!(r.status(), 401, "{api}");
            let r = call(&fx, at_a(api), "sdw_not-a-token", &ask).await;
            assert_eq!(r.status(), 401, "{api}");
        }
        assert!(up.seen().is_empty(), "nothing reached the provider");
        assert!(model_calls(&fx, &a.branch_id).await.is_empty());
    })
}

/// Shaked's own loop in a worker calls `chat/completions` the same way.
#[test]
fn chat_completions_take_a_worker_token_too() {
    run(async {
        let up = fake_apis().await;
        let fx = control("chat", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let ask = json!({"model": "gpt-x", "messages": [{"role": "user", "content": "hi"}]});
        let r = call(
            &fx,
            (&plan.branch_id, "o", "chat/completions"),
            &token,
            &ask,
        )
        .await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.json::<Value>().await.unwrap(), chat_answer());
        bears_the_orgs_router_key(&up.seen()[0]);
        let calls = exactly(&fx, &plan.branch_id, 1).await;
        assert_eq!(calls[0].payload["usage"]["total_tokens"], 11);
    })
}

/// The message the streamed Messages answer builds.
fn streamed_message() -> Value {
    json!({"id": "msg_1", "type": "message", "role": "assistant", "model": "claude-sonnet-5-5",
           "content": [{"type": "text", "text": "hello from claude"}], "stop_reason": "end_turn",
           "usage": {"input_tokens": 12, "cache_read_input_tokens": 30,
                     "cache_creation_input_tokens": 0, "output_tokens": 5}})
}

/// The first chunk of the stream `r`, which must come while the upstream waits on its gate.
async fn first_chunk(r: &mut reqwest::Response) -> String {
    let first = tokio::time::timeout(Duration::from_secs(10), r.chunk()).await;
    let first = first.expect("the first event comes while the upstream waits");
    String::from_utf8(first.unwrap().unwrap().to_vec()).unwrap()
}

#[test]
fn messages_pass_through_streaming() {
    run(async {
        let up = fake_apis().await;
        let fx = control("mstream", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let ask = json!({"model": "claude-sonnet-5-5", "max_tokens": 64, "stream": true,
                         "messages": [{"role": "user", "content": "hi"}]});
        let mut r = call(&fx, (&plan.branch_id, "a", "messages"), &token, &ask).await;
        assert_eq!(r.status(), 200);
        let first = first_chunk(&mut r).await;
        assert_eq!(first, MESSAGE_START);
        assert!(model_calls(&fx, &plan.branch_id).await.is_empty());
        up.gate.add_permits(1);
        let whole = format!("{first}{}", rest_of(&mut r).await);
        assert_eq!(whole, format!("{MESSAGE_START}{MESSAGE_REST}"));
        let calls = exactly(&fx, &plan.branch_id, 1).await;
        let p = &calls[0].payload;
        assert_eq!(
            (&p["response"], &p["stream"]),
            (&streamed_message(), &json!(true))
        );
        let u = &p["usage"];
        let tokens = (
            &u["prompt_tokens"],
            &u["completion_tokens"],
            &u["input_tokens"],
        );
        assert_eq!(tokens, (&json!(42), &json!(5), &json!(12)));
    })
}

/// A caller that leaves after the first chunk: the stream is still read to its end and
/// recorded once, whole.
#[test]
fn a_stream_its_caller_left_is_recorded_whole() {
    run(async {
        let up = fake_apis().await;
        let fx = control("left", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let ask = json!({"model": "gpt-x", "stream": true, "input": "hi"});
        let mut r = call(&fx, (&plan.branch_id, "c", "responses"), &token, &ask).await;
        assert!(first_chunk(&mut r).await.contains("response.created"));
        drop(r);
        up.gate.add_permits(1);
        let calls = exactly(&fx, &plan.branch_id, 1).await;
        recorded_whole(&calls[0].payload);
        assert!(calls[0].payload.get("interrupted").is_none());
        assert_eq!(
            calls[0].payload["charged_usd"], 0.004,
            "charged what it reported"
        );
    })
}

/// A stream the provider breaks off is recorded with what came and why it stopped.
#[test]
fn a_cut_stream_is_recorded_as_interrupted() {
    run(async {
        let up = fake_apis().await;
        let fx = control("cut", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let ask = json!({"model": "cut", "stream": true, "input": "hi"});
        let mut r = call(&fx, (&plan.branch_id, "c", "responses"), &token, &ask).await;
        assert!(first_chunk(&mut r).await.contains("response.created"));
        up.gate.add_permits(1);
        assert!(r.chunk().await.is_err(), "the caller sees the stream fail");
        let calls = exactly(&fx, &plan.branch_id, 1).await;
        let p = &calls[0].payload;
        let why = p["interrupted"].as_str().unwrap_or_default();
        assert!(why.contains("stream stopped"), "{p}");
        assert_eq!(p["response"]["events"][0]["type"], "response.created");
    })
}

/// What the control box sends the link next, once it has something to say.
async fn told(ws: &mut Ws) -> ToWorker {
    next_frame(ws).await
}

/// The control box keeps a linked run to its limit: each call is charged with what it cost, and
/// once over, every call is refused (402) before it reaches a provider, the limit is recorded
/// once and the worker is told to stop.
#[test]
fn a_linked_run_is_refused_past_its_spend_limit() {
    run(async {
        let up = fake_apis().await;
        let fx = control("limit", &up).await;
        let rules = Rules::new(Runner::Chain).with_max_cost(Some(0.01));
        let (plan, token, mut link) = worker_branch_under(&fx, rules).await;
        let ask = json!({"model": "gpt-x", "messages": [{"role": "user", "content": "hi"}]});
        let chat = (plan.branch_id.as_str(), "o", "chat/completions");
        for _ in 0..3 {
            assert_eq!(call(&fx, chat, &token, &ask).await.status(), 200);
        }
        for route in [chat, (plan.branch_id.as_str(), "a", "messages")] {
            let r = call(&fx, route, &token, &ask).await;
            assert_eq!(r.status(), 402, "{}", route.2);
        }
        assert_eq!(
            up.seen().len(),
            3,
            "no call past the limit reached the provider"
        );
        assert!(matches!(told(&mut link).await, ToWorker::OverBudget { .. }));
        let evs = records(&fx.state.store)
            .events(&plan.branch_id)
            .await
            .unwrap();
        let limit =
            |e: &&Event| e.kind == EventKind::WorldEvent && e.payload["kind"] == "limit_reached";
        assert_eq!(evs.iter().filter(limit).count(), 1, "recorded once");
        let charged = |e: &Event| e.payload["charged_usd"].as_f64();
        assert_eq!(evs.iter().filter_map(charged).count(), 3);
    })
}

/// A paused linked branch's calls are refused (409, saying `paused`) until it resumes.
#[test]
fn a_paused_linked_branch_refuses_its_calls() {
    run(async {
        let up = fake_apis().await;
        let fx = control("paused", &up).await;
        let (plan, token, mut link) = worker_branch(&fx).await;
        let id = &plan.branch_id;
        let (status, v) = fx
            .post(&format!("/api/branches/{id}/pause"), json!({}))
            .await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(told(&mut link).await, ToWorker::Pause);
        let ask = json!({"model": "claude-sonnet-5-5", "max_tokens": 8, "messages": []});
        let r = call(&fx, (id, "a", "messages"), &token, &ask).await;
        assert_eq!(r.status(), 409);
        assert_eq!(r.headers()["x-shakedown-held"], "paused");
        assert!(up.seen().is_empty());
        let (status, v) = fx
            .post(&format!("/api/branches/{id}/resume"), json!({}))
            .await;
        assert_eq!(status, 200, "{v}");
        let r = call(&fx, (id, "a", "messages"), &token, &ask).await;
        assert_eq!(r.status(), 200);
    })
}

/// On a linked branch: an agent not in its team is 404, a model without a price is refused 400
/// (its cost could not be held to the limit), and a provider no key names is recorded as a
/// failed call (502), as `chat/completions` records it.
#[test]
fn a_linked_branch_refuses_strangers_unpriced_models_and_records_unreached_providers() {
    run(async {
        let up = fake_apis().await;
        let fx = control("refusals", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let id = plan.branch_id.as_str();
        let ask = json!({"model": "claude-sonnet-5-5", "max_tokens": 8, "messages": []});
        let r = call(&fx, (id, "stranger", "messages"), &token, &ask).await;
        assert_eq!(r.status(), 404);
        let unpriced = json!({"model": "claude-unlisted-9", "max_tokens": 8, "messages": []});
        let r = call(&fx, (id, "a", "messages"), &token, &unpriced).await;
        assert_eq!(r.status(), 400);
        let why = r.json::<Value>().await.unwrap()["error"].to_string();
        assert!(why.contains("claude-unlisted-9"), "{why}");
        assert!(up.seen().is_empty());
        let r = call(&fx, (id, "k", "messages"), &token, &ask).await;
        assert_eq!(r.status(), 502);
        let calls = exactly(&fx, id, 1).await;
        let error = calls[0].payload["response"]["error"].to_string();
        assert!(error.contains("key:nope"), "{error}");
    })
}

/// `count_tokens` goes to the provider with the org's key and is neither recorded nor charged.
#[test]
fn count_tokens_passes_through_unrecorded() {
    run(async {
        let up = fake_apis().await;
        let fx = control("count", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let ask = json!({"model": "claude-sonnet-5-5", "messages": []});
        let route = (plan.branch_id.as_str(), "a", "messages/count_tokens");
        let r = call(&fx, route, &token, &ask).await;
        assert_eq!(r.status(), 200);
        assert_eq!(r.json::<Value>().await.unwrap(), json!({"input_tokens": 7}));
        let seen = up.seen();
        assert_eq!(seen[0].path, "/v1/messages/count_tokens");
        assert_eq!(seen[0].headers["x-api-key"], ORG_CLAUDE_KEY);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(model_calls(&fx, &plan.branch_id).await.is_empty());
    })
}

/// A worker's `RunStart` event as a tampered worker might write it: `extra` its payload.
fn run_start(extra: Value) -> ToControl {
    ToControl::Event {
        n: 1,
        agent: None,
        kind: EventKind::RunStart,
        payload: extra,
        overhead: None,
    }
}

/// Sends the worker's `RunStart` `extra` over `link` and waits for its ack.
async fn wrote_run_start(link: &mut Ws, extra: Value) {
    say(link, &run_start(extra)).await;
    assert!(matches!(next_frame(link).await, ToWorker::Ack { n: 1, .. }));
}

fn chat_ask() -> Value {
    json!({"model": "gpt-x", "messages": [{"role": "user", "content": "hi"}]})
}

/// `n` chat calls of agent `o` on `branch` at `base`, each answered `status`.
async fn chats(base: &str, branch: &str, token: &str, n: usize, status: u16) {
    for _ in 0..n {
        let r = call_on(base, (branch, "o", "chat/completions"), token, &chat_ask()).await;
        assert_eq!(r.status(), status);
    }
}

/// A grace no test waits out.
const LONG: Duration = Duration::from_secs(30);

fn limited(usd: f64) -> Rules {
    Rules::new(Runner::Chain).with_max_cost(Some(usd))
}

/// The limit the control box planned counts, not the one a worker writes in its `RunStart`.
#[test]
fn a_worker_written_limit_counts_for_nothing() {
    run(async {
        let up = fake_apis().await;
        let fx = control("tampered", &up).await;
        let (plan, token, mut link) = worker_branch_under(&fx, limited(0.01)).await;
        wrote_run_start(&mut link, json!({"max_cost_usd": 1_000_000.0})).await;
        chats(&fx.base, &plan.branch_id, &token, 3, 200).await;
        chats(&fx.base, &plan.branch_id, &token, 1, 402).await;
        assert_eq!(up.seen().len(), 3);
    })
}

/// After a control-box restart the link is rebuilt from what the store kept: the limit the
/// control box planned (never the worker's `RunStart`, here forged high) and what it charged.
#[test]
fn a_restarted_control_box_keeps_the_limit_and_the_spend() {
    run(async {
        let up = fake_apis().await;
        let fx = control("restart", &up).await;
        let (plan, token, mut link) = worker_branch_under(&fx, limited(0.01)).await;
        wrote_run_start(&mut link, json!({"max_cost_usd": 1_000_000.0})).await;
        let id = plan.branch_id.as_str();
        chats(&fx.base, id, &token, 2, 200).await;
        drop(link);
        let (_again, base) = restarted(&fx, &up, LONG).await;
        let _link = rejoined(&base, &token, 1).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        chats(&base, id, &token, 1, 200).await;
        chats(&base, id, &token, 1, 402).await;
        assert_eq!(up.seen().len(), 3, "the restart forgot none of the spend");
    })
}

/// A linked branch whose rules are not on record is refused, never unlimited.
#[test]
fn a_linked_branch_without_rules_on_record_is_refused() {
    run(async {
        let up = fake_apis().await;
        let fx = control("norules", &up).await;
        let (plan, token, link) = worker_branch(&fx).await;
        let id = plan.branch_id.as_str();
        let sql = format!("UPDATE shakedown.branches SET rules = NULL WHERE id = '{id}'");
        support::pg::execute(&fx.db, &sql).await;
        drop(link);
        let (_again, base) = restarted(&fx, &up, LONG).await;
        let _link = rejoined(&base, &token, 0).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let r = call_on(&base, (id, "o", "chat/completions"), &token, &chat_ask()).await;
        assert_eq!(r.status(), 503);
        let why = r.json::<Value>().await.unwrap()["error"].to_string();
        assert!(why.contains("no limits on record"), "{why}");
        assert!(up.seen().is_empty());
    })
}

/// The status of branch `id` on record.
async fn status(fx: &AppFx, id: &str) -> BranchStatus {
    let b = records(&fx.state.store).find_branch(id).await.unwrap();
    b.unwrap().status
}

/// Branch `id`'s status once it is no longer running (within 10 s).
async fn ended(fx: &AppFx, id: &str) -> BranchStatus {
    for _ in 0..100 {
        let now = status(fx, id).await;
        if now != BranchStatus::Running {
            return now;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("branch {id} is still running after 10 s");
}

/// Waits until `fx` sees the worker of `id` gone (within 5 s).
async fn disconnected(fx: &AppFx, id: &str) {
    for _ in 0..50 {
        if !fx.state.links.connected(id) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the worker of {id} is still connected");
}

/// A pause pressed while the worker is away is kept, enforced on its calls, and told to the
/// worker when it rejoins.
#[test]
fn a_pause_while_the_worker_is_away_is_enforced_and_sent_again_on_rejoin() {
    run(async {
        let up = fake_apis().await;
        let fx = control("away", &up).await;
        let (plan, token, link) = worker_branch(&fx).await;
        let id = plan.branch_id.as_str();
        drop(link);
        disconnected(&fx, id).await;
        let (code, v) = fx
            .post(&format!("/api/branches/{id}/pause"), json!({}))
            .await;
        assert_eq!((code, v), (200, json!({"paused": true, "changed": true})));
        let r = call(&fx, (id, "o", "chat/completions"), &token, &chat_ask()).await;
        assert_eq!(r.status(), 409);
        let mut link = rejoined(&fx.base, &token, 0).await;
        assert_eq!(next_frame(&mut link).await, ToWorker::Pause);
        assert!(up.seen().is_empty());
    })
}

/// A stop pressed while no worker is connected ends the branch at once, and its worker token
/// opens nothing any more.
#[test]
fn a_stop_while_no_worker_is_connected_ends_the_branch_at_once() {
    run(async {
        let up = fake_apis().await;
        let fx = control("gone", &up).await;
        let (plan, token, link) = worker_branch(&fx).await;
        let id = plan.branch_id.as_str();
        drop(link);
        disconnected(&fx, id).await;
        let (code, v) = fx
            .post(&format!("/api/branches/{id}/stop"), json!({}))
            .await;
        assert_eq!((code, v), (200, json!({"stopping": true})));
        assert_eq!(status(&fx, id).await, BranchStatus::Stopped);
        let r = call(&fx, (id, "o", "chat/completions"), &token, &chat_ask()).await;
        assert_eq!(r.status(), 401);
    })
}

/// A worker that ignores `OverBudget` is ended by the control box after the grace,
/// `budget_exceeded`: its connection is told so and closed, its token opens nothing any more,
/// and the branch takes no more of its events.
#[test]
fn a_worker_that_ignores_over_budget_still_ends_budget_exceeded() {
    run(async {
        let up = fake_apis().await;
        let fx = control_with("ignored", &up, Duration::from_secs(1)).await;
        let (plan, token, mut link) = worker_branch_under(&fx, limited(0.005)).await;
        let id = plan.branch_id.as_str();
        chats(&fx.base, id, &token, 2, 200).await;
        chats(&fx.base, id, &token, 1, 402).await;
        assert!(matches!(
            next_frame(&mut link).await,
            ToWorker::OverBudget { .. }
        ));
        assert_eq!(ended(&fx, id).await, BranchStatus::BudgetExceeded);
        refused_and_closed(&mut link).await;
        chats(&fx.base, id, &token, 1, 401).await;
        no_more_events(&fx, id).await;
    })
}

/// The link is told its branch ended (`Refused`), then closed.
async fn refused_and_closed(link: &mut Ws) {
    match next_frame(link).await {
        ToWorker::Refused { why } => assert!(why.starts_with(REFUSED_ENDED), "{why}"),
        other => panic!("refused, not {other:?}"),
    }
    let next = tokio::time::timeout(Duration::from_secs(10), link.next()).await;
    let closed = matches!(next.unwrap(), None | Some(Ok(Message::Close(_)) | Err(_)));
    assert!(closed, "the connection closes");
}

/// A worker's event sent after its branch ended.
fn late_event() -> Linked {
    Linked {
        n: 1,
        agent: None,
        kind: EventKind::Message,
        payload: json!({"text": "after the end"}),
        overhead: None,
    }
}

/// The ended branch `id` takes no more of its worker's events.
async fn no_more_events(fx: &AppFx, id: &str) {
    let appended = records(&fx.state.store)
        .append_linked(id, late_event())
        .await
        .unwrap();
    assert!(appended.is_none(), "an ended branch takes no more events");
}

/// A worker told to stop that reports `passed` ends `stopped`: the control box's word counts. Its
/// calls are refused while it stops.
#[test]
fn a_worker_that_reports_passed_after_a_stop_ends_stopped() {
    run(async {
        let up = fake_apis().await;
        let fx = control("overruled", &up).await;
        let (plan, token, mut link) = worker_branch(&fx).await;
        let id = plan.branch_id.as_str();
        let (code, _) = fx
            .post(&format!("/api/branches/{id}/stop"), json!({}))
            .await;
        assert_eq!(code, 200);
        assert_eq!(next_frame(&mut link).await, ToWorker::Stop);
        chats(&fx.base, id, &token, 1, 409).await;
        let passed = ToControl::Ended {
            status: BranchStatus::Passed,
            error: None,
        };
        say(&mut link, &passed).await;
        assert_eq!(ended(&fx, id).await, BranchStatus::Stopped);
    })
}

/// Calls through `messages` and through `responses` are charged with what they cost, and reach
/// the run's limit as `chat/completions` calls do.
#[test]
fn messages_and_responses_charges_reach_the_limit() {
    run(async {
        let up = fake_apis().await;
        up.gate.add_permits(10);
        let fx = control("charges", &up).await;
        let (plan, token, _a) = worker_branch_under(&fx, limited(0.0001)).await;
        let ask = json!({"model": "claude-sonnet-5-5", "max_tokens": 8, "messages": []});
        let messages = (plan.branch_id.as_str(), "a", "messages");
        for want in [200, 200, 402] {
            assert_eq!(call(&fx, messages, &token, &ask).await.status(), want);
        }
        let (plan, token, _c) = worker_branch_under(&fx, limited(0.005)).await;
        let ask = json!({"model": "gpt-x", "stream": true, "input": "hi"});
        let responses = (plan.branch_id.as_str(), "c", "responses");
        for want in [200, 200, 402] {
            let mut r = call(&fx, responses, &token, &ask).await;
            assert_eq!(r.status(), want);
            rest_of(&mut r).await;
        }
        asked_openrouter_for_its_cost(&up.seen());
    })
}

/// The upstream got two Responses calls, each asking OpenRouter to report its cost.
fn asked_openrouter_for_its_cost(seen: &[Seen]) {
    let responses: Vec<&Seen> = seen.iter().filter(|s| s.path == "/v1/responses").collect();
    assert_eq!(responses.len(), 2);
    for s in responses {
        assert_eq!(s.body["usage"], json!({"include": true}));
    }
}

/// A branch the control box was stopping when it restarted, with no worker coming back, is ended
/// `stopped` by the restarted one: its sweep arms the grace again.
#[test]
fn a_stop_a_restart_interrupted_still_ends_the_branch() {
    run(async {
        let up = fake_apis().await;
        let fx = control("restop", &up).await;
        let (plan, _token, mut link) = worker_branch(&fx).await;
        let id = plan.branch_id.as_str();
        let (code, _) = fx
            .post(&format!("/api/branches/{id}/stop"), json!({}))
            .await;
        assert_eq!(code, 200);
        assert_eq!(next_frame(&mut link).await, ToWorker::Stop);
        drop(link);
        let (again, _base) = restarted(&fx, &up, Duration::from_millis(300)).await;
        holds::sweep(&again).await.unwrap();
        assert_eq!(ended(&fx, id).await, BranchStatus::Stopped);
    })
}

/// The hold on record of branch `id`.
async fn hold_on_record(fx: &AppFx, id: &str) -> Option<String> {
    let sql = format!("SELECT hold FROM shakedown.branches WHERE id = '{id}'");
    support::pg::query_text(&fx.db, &sql).await
}

/// A limit recorded reached whose hold was lost (the control box went down between the two) is
/// put on that hold again when the link is rebuilt, and not recorded twice.
#[test]
fn a_restart_restores_a_limit_recorded_reached() {
    run(async {
        let up = fake_apis().await;
        let fx = control("refused", &up).await;
        let (plan, token, link) = worker_branch_under(&fx, limited(0.005)).await;
        let id = plan.branch_id.as_str();
        chats(&fx.base, id, &token, 2, 200).await;
        assert_eq!(
            hold_on_record(&fx, id).await.as_deref(),
            Some("budget_exceeded")
        );
        let lost = format!("UPDATE shakedown.branches SET hold = NULL WHERE id = '{id}'");
        support::pg::execute(&fx.db, &lost).await;
        drop(link);
        let (_again, base) = restarted(&fx, &up, Duration::from_millis(300)).await;
        let mut link = rejoined(&base, &token, 0).await;
        assert!(matches!(
            next_frame(&mut link).await,
            ToWorker::OverBudget { .. }
        ));
        assert_eq!(
            hold_on_record(&fx, id).await.as_deref(),
            Some("budget_exceeded")
        );
        assert_eq!(ended(&fx, id).await, BranchStatus::BudgetExceeded);
        let evs = records(&fx.state.store).events(id).await.unwrap();
        let limit = |e: &&Event| e.payload["kind"] == "limit_reached";
        assert_eq!(evs.iter().filter(limit).count(), 1, "recorded once");
    })
}

/// A Messages stream that says `error` and ends is cut: charged the conservative estimate (its
/// input, and the largest output its request allows, at the highest listed prices).
#[test]
fn a_messages_stream_that_errs_is_charged_the_ceiling() {
    run(async {
        let up = fake_apis().await;
        up.gate.add_permits(1);
        let fx = control("errs", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let ask = json!({"model": "claude-sonnet-5-5", "max_tokens": 64, "stream": true,
                         "metadata": {"fake": "error"}, "messages": []});
        let mut r = call(&fx, (&plan.branch_id, "a", "messages"), &token, &ask).await;
        rest_of(&mut r).await;
        let calls = exactly(&fx, &plan.branch_id, 1).await;
        let p = &calls[0].payload;
        assert!(
            p["interrupted"].as_str().unwrap().contains("Overloaded"),
            "{p}"
        );
        let (inp, out) = shaked::prices::table().highest("anthropic").unwrap();
        let ceiling = (42.0 * inp + 64.0 * out) / 1e6;
        let charged = p["charged_usd"].as_f64().unwrap();
        assert!((charged - ceiling).abs() < 1e-12, "{charged} vs {ceiling}");
        assert_eq!(p["estimated"], true);
    })
}

/// A Responses stream that ends without its final event is cut. Its provider (OpenRouter) and its
/// model (no `vendor/` to look up) give no prices to bound it with, so a run with a limit is put
/// over it at once, never charged nothing.
#[test]
fn a_cut_stream_without_a_bound_puts_a_limited_run_over() {
    run(async {
        let up = fake_apis().await;
        up.gate.add_permits(1);
        let fx = control("short", &up).await;
        let (plan, token, mut link) = worker_branch_under(&fx, limited(10.0)).await;
        let id = plan.branch_id.as_str();
        short_stream(&fx, id, &token).await;
        assert!(matches!(
            next_frame(&mut link).await,
            ToWorker::OverBudget { .. }
        ));
        assert_eq!(
            hold_on_record(&fx, id).await.as_deref(),
            Some("budget_exceeded")
        );
        chats(&fx.base, id, &token, 1, 402).await;
    })
}

/// The same cut stream on a run without a limit is not a reason to stop it.
#[test]
fn a_cut_stream_without_a_bound_leaves_an_unlimited_run_alone() {
    run(async {
        let up = fake_apis().await;
        up.gate.add_permits(1);
        let fx = control("unlimited", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let id = plan.branch_id.as_str();
        short_stream(&fx, id, &token).await;
        assert_eq!(hold_on_record(&fx, id).await, None);
        chats(&fx.base, id, &token, 1, 200).await;
    })
}

/// A streamed Responses call on `id` that ends without its final event, recorded as cut.
async fn short_stream(fx: &AppFx, id: &str, token: &str) {
    let ask = json!({"model": "gpt-x", "stream": true, "metadata": {"fake": "short"},
                     "input": "hi"});
    let mut r = call(fx, (id, "c", "responses"), token, &ask).await;
    rest_of(&mut r).await;
    let calls = exactly(fx, id, 1).await;
    let why = calls[0].payload["interrupted"].as_str().unwrap_or_default();
    assert!(why.contains("without its final event"), "{why}");
}

/// A running, unpaused linked branch whose worker token died is ended `infra_error` by the sweep.
#[test]
fn the_sweep_ends_a_branch_whose_worker_token_died() {
    run(async {
        let up = fake_apis().await;
        let fx = control("died", &up).await;
        let (plan, _token, _link) = worker_branch(&fx).await;
        let id = plan.branch_id.as_str();
        expire_worker_token(&fx, id, "1 hour").await;
        holds::sweep(&fx.state).await.unwrap();
        let b = records(&fx.state.store)
            .find_branch(id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(b.status, BranchStatus::InfraError);
        assert_eq!(b.error.as_deref(), Some(holds::TOKEN_DIED));
    })
}

/// Branch `id`'s worker token expired `ago`.
async fn expire_worker_token(fx: &AppFx, id: &str, ago: &str) {
    let sql = format!(
        "UPDATE shakedown.run_tokens SET expires_at = now() - interval '{ago}'
          WHERE branch_id = '{id}' AND kind = 'worker'"
    );
    support::pg::execute(&fx.db, &sql).await;
}

/// Branch `id` paused two hours ago, its worker token's wall-clock expiry an hour behind: a pause
/// that outlasted the token.
async fn paused_past_its_token(fx: &AppFx, id: &str) {
    let (code, _) = fx
        .post(&format!("/api/branches/{id}/pause"), json!({}))
        .await;
    assert_eq!(code, 200);
    let began = format!(
        "UPDATE shakedown.branches SET paused_at = now() - interval '2 hours' WHERE id = '{id}'"
    );
    support::pg::execute(&fx.db, &began).await;
    expire_worker_token(fx, id, "1 hour").await;
}

/// A pause that outlasted the worker token's wall-clock expiry survives a real restart (its
/// startup interrupt and sweep): the token counts in unpaused time, so the worker rejoins before
/// the resume, the resume extends the token by the pause, and the worker goes on.
#[test]
fn a_long_pause_survives_a_restart_and_its_worker_rejoins() {
    run(async {
        let up = fake_apis().await;
        let fx = control("longpause", &up).await;
        let (plan, token, link) = worker_branch(&fx).await;
        let id = plan.branch_id.as_str();
        paused_past_its_token(&fx, id).await;
        drop(link);
        shaked::jobs::interrupt_at_start(&fx.state.store)
            .await
            .unwrap();
        let (again, base) = restarted(&fx, &up, LONG).await;
        holds::sweep(&again).await.unwrap();
        assert_eq!(status(&fx, id).await, BranchStatus::Running);
        let mut link = rejoined(&base, &token, 0).await;
        assert_eq!(
            next_frame(&mut link).await,
            ToWorker::Pause,
            "rejoined, still paused"
        );
        let resume = reqwest::Client::new().post(format!("{base}/api/branches/{id}/resume"));
        let (code, v) = app::send(resume.json(&json!({}))).await;
        assert_eq!((code, v), (200, json!({"paused": false, "changed": true})));
        assert_eq!(next_frame(&mut link).await, ToWorker::Resume);
        let live = resolve_run_token(&fx.state.store, &token).await.unwrap();
        assert!(live.is_some(), "the resume extended the token by the pause");
        chats(&base, id, &token, 1, 200).await;
    })
}

/// An end and an append race on the branch's lock: the append queued behind the end is refused,
/// never landing after it.
#[test]
fn an_append_racing_an_end_never_lands_after_it() {
    run(async {
        let up = fake_apis().await;
        let fx = control("race", &up).await;
        let (plan, _token, _link) = worker_branch(&fx).await;
        let id = plan.branch_id.clone();
        let lock = fx.state.store.branch_lock(&id);
        let held = lock.lock().await;
        let (state, branch) = (fx.state.clone(), id.clone());
        let ending = tokio::spawn(async move {
            holds::end(
                &state,
                &branch,
                (BranchStatus::Stopped, None),
                EndIf::Running,
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (store, branch) = (fx.state.store.clone(), id.clone());
        let appending =
            tokio::spawn(async move { records(&store).append_linked(&branch, late_event()).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(held);
        ending.await.unwrap().unwrap();
        assert!(
            appending.await.unwrap().unwrap().is_none(),
            "refused after the end"
        );
        assert_eq!(status(&fx, &id).await, BranchStatus::Stopped);
    })
}

/// A body that counts the bytes read of it: `chunks` of 1 MiB.
fn counted_body(chunks: usize, read: Arc<std::sync::atomic::AtomicUsize>) -> axum::body::Body {
    use std::sync::atomic::Ordering;
    let stream = futures_util::stream::iter(0..chunks).map(move |_| {
        read.fetch_add(1 << 20, Ordering::SeqCst);
        Ok::<_, std::io::Error>(vec![b' '; 1 << 20])
    });
    axum::body::Body::from_stream(stream)
}

/// A request with `body` to `route`, `token` its bearer, straight to the app's router.
async fn sent_to_router(
    fx: &AppFx,
    route: (&str, &str, &str),
    token: &str,
    body: axum::body::Body,
) -> u16 {
    use tower::ServiceExt;
    let (branch, agent, api) = route;
    let req = axum::http::Request::post(format!("/gateway/{branch}/{agent}/v1/{api}"))
        .header("authorization", format!("Bearer {token}"))
        .body(body)
        .unwrap();
    let router = shaked::app::router(fx.state.clone());
    router.oneshot(req).await.unwrap().status().as_u16()
}

/// A call nobody may make never has its body read: a 40 MiB body with a bogus worker token is
/// 401 at once. One past its API's limit is 413, saying so as every refusal does.
#[test]
fn a_body_is_read_only_once_the_call_is_let_through() {
    run(async {
        let up = fake_apis().await;
        let fx = control("bodies", &up).await;
        let (plan, token, _link) = worker_branch(&fx).await;
        let read = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let route = (plan.branch_id.as_str(), "a", "messages");
        let body = counted_body(40, read.clone());
        assert_eq!(sent_to_router(&fx, route, "sdw_bogus", body).await, 401);
        assert_eq!(
            read.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "not a byte read"
        );
        let body = counted_body(33, read.clone());
        assert_eq!(sent_to_router(&fx, route, &token, body).await, 413);
        let r = call(&fx, route, &token, &json!({"pad": "x".repeat(33 << 20)})).await;
        assert_eq!(r.status(), 413);
        let why = r.json::<Value>().await.unwrap()["error"].to_string();
        assert!(why.contains("32 MiB"), "{why}");
        assert!(up.seen().is_empty());
    })
}
