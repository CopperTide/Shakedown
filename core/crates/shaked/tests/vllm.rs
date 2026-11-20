//! An external run's agent on a vLLM gpt-oss server behind the gateway (a scripted fake server
//! shaped like vLLM v0.32.0's answers; no real model): its reasoning is recorded once per call,
//! the server's own key goes upstream, what the agent sends reaches the server as sent, a rated
//! server's calls are priced, and a proxy timeout is recorded and passed on.
mod support;
use support::records;

use serde_json::{json, Value};
use shaked::model::*;
use shaked::providers::{Endpoint, LocalEndpoint, ProviderConfig, Rate, Secret};
use support::fake_llm::{fake_llm, gpt_oss, FakeLlm};
use support::{run, AppFx};

/// The providers: only the local server `pod` at `base`, with its key and `rate`.
fn pod(base: &str, rate: Option<Rate>) -> ProviderConfig {
    let mut c = ProviderConfig::default();
    c.locals.push(LocalEndpoint {
        name: "pod".into(),
        endpoint: Endpoint::new(base, Some(Secret::new("k-pod"))),
        rate,
    });
    c.default = "local:pod".into();
    c
}

/// An app on the server, an external run of task 27 with agent `a` on `local:pod`: the app,
/// the branch and `a`'s gateway URL.
async fn external_on(tag: &str, llm: &FakeLlm, rate: Option<Rate>) -> (AppFx, String, String) {
    let fx = AppFx::idle("vllm", tag)
        .providers(pod(&llm.base, rate))
        .start()
        .await;
    let team = json!({"id": "own", "name": "Own", "agents": []});
    let body = json!({"task_id": "27", "team": team, "runner": "external"});
    let (status, v) = fx.post("/api/runs", body).await;
    assert_eq!(status, 200, "{v}");
    let branch = v["branch_id"].as_str().unwrap().to_string();
    let join = json!({"id": "a", "provider": "local:pod", "model": "openai/gpt-oss-120b"});
    let (status, reg) = fx
        .post(&format!("/api/branches/{branch}/agents"), join)
        .await;
    assert_eq!(status, 200, "{reg}");
    let gateway = reg["gateway_url"].as_str().unwrap().to_string();
    (fx, branch, gateway)
}

/// One chat call through the gateway with the run's bearer token: status and answer.
async fn chat(fx: &AppFx, gateway: &str, body: Value) -> (u16, Value) {
    let req = fx.http.post(format!("{gateway}/chat/completions"));
    // The agent `a`'s token, from its live branch (`<base>/gateway/<branch>/a/v1`).
    let branch = gateway
        .split("/gateway/")
        .nth(1)
        .and_then(|r| r.split('/').next())
        .unwrap();
    let token = fx.state.live.get(branch).unwrap().agent_token("a");
    let r = req.bearer_auth(token).json(&body).send().await.unwrap();
    let status = r.status().as_u16();
    (status, r.json().await.unwrap_or(Value::Null))
}

fn ask(messages: Value) -> Value {
    json!({"model": "openai/gpt-oss-120b", "messages": messages, "max_tokens": 4096})
}

fn user(text: &str) -> Value {
    json!([{"role": "user", "content": text}])
}

/// `a`'s events of `kinds`, in order.
async fn of(fx: &AppFx, branch: &str, kinds: &[EventKind]) -> Vec<Event> {
    let evs = records(&fx.state.store).events(branch).await.unwrap();
    let mine = |e: &Event| e.agent.as_deref() == Some("a") && kinds.contains(&e.kind);
    evs.into_iter().filter(mine).collect()
}

const CALLS: [EventKind; 2] = [EventKind::ModelCall, EventKind::Reasoning];

#[test]
fn a_vllm_gpt_oss_answer_records_reasoning_for_each_call() {
    run(async {
        let first = gpt_oss("Look at the sprint first.", None);
        let llm = fake_llm(vec![
            (200, first.clone()),
            (200, gpt_oss("PR is open.", Some("Done."))),
        ])
        .await;
        let (fx, branch, gw) = external_on("reason", &llm, None).await;
        let (status, answer) = chat(&fx, &gw, ask(user("go"))).await;
        assert_eq!(
            (status, &answer),
            (200, &first),
            "the agent gets the answer unchanged"
        );
        chat(&fx, &gw, ask(user("again"))).await;
        let evs = of(&fx, &branch, &CALLS).await;
        let kinds: Vec<EventKind> = evs.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [CALLS[0], CALLS[1], CALLS[0], CALLS[1]]);
        for (i, text) in ["Look at the sprint first.", "PR is open."]
            .iter()
            .enumerate()
        {
            let (call, thought) = (&evs[2 * i], &evs[2 * i + 1]);
            // `truncated` is left out when false (as for an agent's own reports)
            let want = json!({"text": text, "kind": "thinking", "source": "gateway",
                              "model_call_seq": call.seq});
            assert_eq!(support::written(&thought.payload), want);
        }
    })
}

#[test]
fn older_vllm_reasoning_content_is_recorded_once() {
    run(async {
        let mut older = gpt_oss("r", None);
        let message = older["choices"][0]["message"].as_object_mut().unwrap();
        message.remove("reasoning");
        message.insert("reasoning_content".into(), json!("r"));
        let mut both = older.clone();
        both["choices"][0]["message"]["reasoning"] = json!("r");
        let llm = fake_llm(vec![(200, older), (200, both)]).await;
        let (fx, branch, gw) = external_on("older", &llm, None).await;
        chat(&fx, &gw, ask(user("1"))).await;
        chat(&fx, &gw, ask(user("2"))).await;
        let thoughts = of(&fx, &branch, &[EventKind::Reasoning]).await;
        let texts: Vec<&Value> = thoughts.iter().map(|e| &e.payload["text"]).collect();
        assert_eq!(texts, [&json!("r"), &json!("r")], "one item per call");
    })
}

#[test]
fn the_pods_key_goes_upstream_and_the_agents_token_does_not() {
    run(async {
        let llm = fake_llm(vec![(200, gpt_oss("t", None))]).await;
        let (fx, _, gw) = external_on("key", &llm, None).await;
        chat(&fx, &gw, ask(user("go"))).await;
        let seen = llm.seen.lock().unwrap()[0].clone();
        assert_eq!(seen.authorization.as_deref(), Some("Bearer k-pod"));
        assert!(seen.path.ends_with("/v1/chat/completions"), "{}", seen.path);
    })
}

#[test]
fn prior_reasoning_in_the_request_reaches_the_server_unchanged() {
    run(async {
        let llm = fake_llm(vec![(200, gpt_oss("t", None))]).await;
        let (fx, _, gw) = external_on("passback", &llm, None).await;
        let messages = json!([{"role": "user", "content": "go"},
            {"role": "assistant", "content": "ok", "reasoning": "earlier"},
            {"role": "user", "content": "next"}]);
        let mut body = ask(messages.clone());
        body["include_reasoning"] = json!(true);
        body["reasoning_effort"] = json!("medium");
        chat(&fx, &gw, body).await;
        let seen = llm.seen.lock().unwrap()[0].body.clone();
        assert_eq!(seen["messages"], messages);
        assert_eq!(
            (&seen["include_reasoning"], &seen["reasoning_effort"]),
            (&json!(true), &json!("medium"))
        );
    })
}

#[test]
fn a_rated_local_server_prices_calls() {
    run(async {
        let llm = fake_llm(vec![(200, gpt_oss("t", None))]).await;
        let rate = Rate {
            input_per_mtok: 0.15,
            output_per_mtok: 0.60,
        };
        let (fx, branch, gw) = external_on("rated", &llm, Some(rate)).await;
        chat(&fx, &gw, ask(user("go"))).await;
        let call = &of(&fx, &branch, &[EventKind::ModelCall]).await[0];
        let cost = call.payload["cost_usd"].as_f64().unwrap();
        assert!(
            (cost - (812.0 * 0.15e-6 + 64.0 * 0.60e-6)).abs() < 1e-12,
            "{cost}"
        );
        assert_eq!(
            call.payload["cost_source"],
            "rate set for this local server"
        );
    })
}

#[test]
fn a_cloudflare_timeout_is_recorded_and_passed_on() {
    run(async {
        let html = "<html><body>A timeout occurred (Cloudflare 524)</body></html>";
        let llm = fake_llm(vec![(524, json!(html))]).await;
        let (fx, branch, gw) = external_on("timeout", &llm, None).await;
        let (status, _) = chat(&fx, &gw, ask(user("go"))).await;
        assert_eq!(status, 524);
        let call = &of(&fx, &branch, &[EventKind::ModelCall]).await[0];
        let raw = call.payload["response"]["body"]["raw"]
            .as_str()
            .unwrap_or_default();
        assert!(raw.contains("timeout"), "{}", call.payload);
        assert!(of(&fx, &branch, &[EventKind::Reasoning]).await.is_empty());
    })
}

/// `external_on`, on an app whose store calls `spy`, with the agent on `provider`.
async fn external_spied(
    tag: &str,
    providers: ProviderConfig,
    spy: &shaked::harness::SpyLayer,
    provider: &str,
) -> (AppFx, String, String) {
    let fx = AppFx::idle("vllm", tag)
        .providers(providers)
        .layer(std::sync::Arc::new(spy.clone()))
        .start()
        .await;
    let team = json!({"id": "own", "name": "Own", "agents": []});
    let body = json!({"task_id": "27", "team": team, "runner": "external"});
    let (status, v) = fx.post("/api/runs", body).await;
    assert_eq!(status, 200, "{v}");
    let branch = v["branch_id"].as_str().unwrap().to_string();
    let join = json!({"id": "a", "provider": provider, "model": "openai/gpt-oss-120b"});
    let (status, reg) = fx
        .post(&format!("/api/branches/{branch}/agents"), join)
        .await;
    assert_eq!(status, 200, "{reg}");
    let gateway = reg["gateway_url"].as_str().unwrap().to_string();
    (fx, branch, gateway)
}

#[test]
fn every_recorded_model_call_is_metered_once() {
    use shaked::{harness::Call, layer::Meter};
    run(async {
        let llm = fake_llm(vec![(200, gpt_oss("t", None))]).await;
        let spy = shaked::harness::SpyLayer::new();
        let (fx, branch, gw) =
            external_spied("meter", pod(&llm.base, None), &spy, "local:pod").await;
        let (status, _) = chat(&fx, &gw, ask(user("go"))).await;
        assert_eq!(status, 200);
        let calls = of(&fx, &branch, &[EventKind::ModelCall]).await;
        let key = format!("gateway:{branch}:{}", calls[0].seq);
        let meters: Vec<Call> = spy
            .calls()
            .into_iter()
            .filter(|c| {
                matches!(
                    c,
                    Call::Meter(Meter::ModelUsdMicros | Meter::ModelTokens, _)
                )
            })
            .collect();
        let want = [
            Call::Meter(Meter::ModelUsdMicros, key.clone()),
            Call::Meter(Meter::ModelTokens, format!("{key}:tokens")),
        ];
        assert_eq!(meters, want);
        assert!(spy
            .calls()
            .iter()
            .any(|c| matches!(c, Call::Admit(op) if op.starts_with("ModelCall"))));
    })
}

#[test]
fn an_agent_without_a_key_gets_the_layer_fallback() {
    use shaked::{harness::Call, layer::Fallback};
    run(async {
        let llm = fake_llm(vec![(200, gpt_oss("t", None))]).await;
        let lent = pod(&llm.base, None).resolve(Some("local:pod")).unwrap();
        let spy = shaked::harness::SpyLayer::new().lending(Fallback {
            provider: lent,
            source: "trial",
        });
        let keyless = ProviderConfig::with_upstream(support::app::NOWHERE);
        let (fx, _branch, gw) = external_spied("fallback", keyless, &spy, "openrouter").await;
        let (status, answer) = chat(&fx, &gw, ask(user("go"))).await;
        assert_eq!(status, 200, "{answer}");
        assert_eq!(
            llm.seen.lock().unwrap().len(),
            1,
            "the lent provider answered"
        );
        let asked = Call::Fallback("openai/gpt-oss-120b".into());
        assert!(spy.calls().contains(&asked), "{:?}", spy.calls());
    })
}
