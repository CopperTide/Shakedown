//! A run an external system drives over HTTP: it starts the run, registers its agents, runs each
//! one itself against the agent's MCP endpoint, reports the model calls its client saw, ends the
//! agents and finishes the run; a fork hands it each agent's history. Without a model: what is
//! refused. `live_`: the agent is Claude Code (signed in; see `support/live.rs`), run by this
//! test the way a design in its own code runs it.
mod support;
use support::records;

use serde_json::{json, Value};
use shaked::model::*;
use support::{app::send, live, live::Live, run, run_live, AppFx};

async fn setup(tag: &str) -> AppFx {
    AppFx::idle("external", tag).start().await
}

impl AppFx {
    /// `POST /api/branches/{branch}/{what}`.
    async fn on(&self, branch: &str, what: &str, body: Value) -> (u16, Value) {
        self.post(&format!("/api/branches/{branch}/{what}"), body)
            .await
    }

    async fn finish(&self, branch: &str) -> (u16, Value) {
        self.on(branch, "finish", json!({})).await
    }

    async fn finish_with(&self, branch: &str, body: Value) -> (u16, Value) {
        self.on(branch, "finish", body).await
    }

    async fn events(&self, branch: &str) -> Vec<Event> {
        records(&self.state.store).events(branch).await.unwrap()
    }

    /// The branch's metrics, from `GET /api/branches/{id}`.
    async fn metrics_of(&self, branch: &str) -> Value {
        let url = format!("{}/api/branches/{branch}", self.base);
        let (status, b) = send(self.http.get(url)).await;
        assert_eq!(status, 200, "{b}");
        b["metrics"].clone()
    }
}

fn team(agents: Value) -> Value {
    json!({"id": "own", "name": "Own design", "agents": agents})
}

/// Starts an external run of task 27 with no agents yet: its branch id.
async fn start(fx: &AppFx) -> String {
    let body = json!({"task_id": "27", "team": team(json!([])), "runner": "external"});
    let (status, v) = fx.post("/api/runs", body).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["agents"], json!([]));
    v["branch_id"].as_str().unwrap().to_string()
}

async fn register(fx: &AppFx, branch: &str, join: Value) -> Value {
    let (status, v) = fx.on(branch, "agents", join).await;
    assert_eq!(status, 200, "{v}");
    v
}

fn kinds(evs: &[Event]) -> Vec<EventKind> {
    evs.iter().map(|e| e.kind).collect()
}

#[test]
fn only_external_runs_take_registrations() {
    run(async {
        let fx = setup("refuse").await;
        assert_eq!(fx.on("nope", "agents", json!({"id": "a"})).await.0, 404);
        let branch = start(&fx).await;
        for bad in ["link", "Bad Id"] {
            let (status, v) = fx.on(&branch, "agents", json!({"id": bad})).await;
            assert_eq!(status, 400, "{v}");
        }
        // An external agent can't be run by shaked's own runners.
        let agent =
            json!({"id": "x", "name": "X", "kind": "external", "model": null, "prompt": ""});
        let body = json!({"task_id": "27", "team": team(json!([agent]))});
        let (status, v) = fx.post("/api/runs", body).await;
        assert_eq!(status, 400, "{v}");
        let unknown = fx.on(&branch, "agents/nobody/usage", json!({"input_tokens": 1}));
        assert_eq!(unknown.await.0, 404);
        assert_eq!(fx.finish(&branch).await.0, 200);
        assert_eq!(
            fx.finish(&branch).await.0,
            404,
            "a finished branch is no longer live"
        );
    })
}

/// The payloads of `agent`'s reasoning events.
fn reasoning_of<'a>(evs: &'a [Event], agent: &str) -> Vec<&'a Value> {
    let mine = |e: &&Event| e.kind == EventKind::Reasoning && e.agent.as_deref() == Some(agent);
    evs.iter().filter(mine).map(|e| &e.payload).collect()
}

#[test]
fn an_agent_reports_its_reasoning() {
    run(async {
        let fx = setup("reasoning").await;
        let branch = start(&fx).await;
        register(&fx, &branch, json!({"id": "a"})).await;
        let long = "x".repeat(40_000);
        let body = json!({"items": [
            {"text": "Check stock first.", "kind": "plan"},
            {"text": "   "},
            {"text": long},
        ]});
        let (status, v) = fx.on(&branch, "agents/a/reasoning", body).await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(v["seqs"].as_array().unwrap().len(), 2, "{v}");
        let evs = fx.events(&branch).await;
        let said = reasoning_of(&evs, "a");
        assert_eq!(said.len(), 2);
        let first = &said[0];
        let got = (&first["text"], &first["kind"], &first["source"]);
        let (text, kind, source) = (json!("Check stock first."), json!("plan"), json!("agent"));
        assert_eq!(got, (&text, &kind, &source));
        assert!(first.get("truncated").is_none() && first.get("model_call_seq").is_none());
        assert_eq!(said[1]["truncated"], json!(true));
        assert_eq!(said[1]["text"].as_str().unwrap().chars().count(), 32_768);
        assert_eq!(said[1]["kind"], json!("thinking"));
        assert_eq!(fx.finish(&branch).await.0, 200);
    })
}

#[test]
fn reasoning_reports_are_checked() {
    run(async {
        let fx = setup("reasoning-bad").await;
        let branch = start(&fx).await;
        register(&fx, &branch, json!({"id": "a"})).await;
        let many: Vec<Value> = (0..101).map(|i| json!({"text": format!("t{i}")})).collect();
        for bad in [
            json!({"items": []}),
            json!({"items": many}),
            json!({"items": [{"text": "t", "kind": "musing"}]}),
        ] {
            let (status, v) = fx.on(&branch, "agents/a/reasoning", bad).await;
            assert_eq!(status, 400, "{v}");
        }
        let ok = json!({"items": [{"text": "t"}]});
        let (status, v) = fx.on(&branch, "agents/nobody/reasoning", ok).await;
        assert_eq!(status, 404, "{v}");
        assert_eq!(fx.finish(&branch).await.0, 200);
    })
}

#[test]
fn finish_with_an_error_ends_failed_and_scored() {
    run(async {
        let fx = setup("finish-error").await;
        let branch = start(&fx).await;
        register(&fx, &branch, json!({"id": "a"})).await;
        let (status, v) = fx
            .finish_with(&branch, json!({"error": "driver raised X"}))
            .await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(
            (v["status"].as_str(), v["error"].as_str()),
            (Some("failed"), Some("driver raised X"))
        );
        let url = format!("{}/api/branches/{branch}", fx.base);
        let (_, b) = send(fx.http.get(url)).await;
        assert_eq!(b["status"], "failed", "{b}");
        assert_eq!(b["error"], "driver raised X", "{b}");
        assert!(kinds(&fx.events(&branch).await).contains(&EventKind::Check));
    })
}

// ---------------------------------------------------------------- Claude Code as the agent

/// The ticket of task 27.
async fn ticket(fx: &AppFx) -> String {
    let bench = fx.state.hub.bench_describe("retail").await.unwrap();
    let t = bench.tasks.iter().find(|t| t.id == "27").unwrap();
    t.ticket.clone()
}

/// Registers `id` and runs Claude Code as it on the ticket through its MCP endpoint, reporting
/// each model call the client saw and the agent's end, as a design does. The registration and
/// what was reported.
async fn work(
    fx: &AppFx,
    live: &Live,
    branch: &str,
    join: Value,
    task: &str,
) -> (Value, live::ExternalRun) {
    let id = join["id"].as_str().unwrap().to_string();
    let reg = register(fx, branch, join).await;
    let mcp = reg["mcp_url"].as_str().unwrap();
    let prompt = format!("You are a retail support agent. Use your tools to handle this ticket, then stop.\n\n{task}");
    let ran = live::run_claude(live, "retail", mcp, &prompt).await;
    for r in &ran.reports {
        let (status, v) = fx
            .on(branch, &format!("agents/{id}/usage"), r.clone())
            .await;
        assert_eq!(status, 200, "{v}");
    }
    let ok = ran.result["is_error"] == false;
    let end = json!({"ok": ok, "prompt_tokens": ran.prompt_tokens,
                     "completion_tokens": ran.completion_tokens,
                     "cost_usd": ran.result["total_cost_usd"]});
    assert_eq!(fx.on(branch, &format!("agents/{id}/end"), end).await.0, 200);
    (reg, ran)
}

/// A reported call's whole input: fresh, cache-written and cache-read tokens.
fn whole_input(r: &Value) -> i64 {
    [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
    ]
    .iter()
    .map(|k| r[k].as_i64().unwrap())
    .sum()
}

/// A read-only agent that joins later is offered no write tool, by registration or by its MCP
/// endpoint.
async fn assert_reviewer_reads_only(fx: &AppFx, branch: &str) {
    let reviewer = register(fx, branch, json!({"id": "reviewer", "read_only": true})).await;
    let write = "exchange_delivered_order_items";
    assert!(!reviewer["tools"].to_string().contains(write));
    let offered = live::mcp_tools(reviewer["mcp_url"].as_str().unwrap()).await;
    assert!(!offered.iter().any(|t| t == write), "{offered:?}");
}

/// Finishing the run scores it and records its check, after everything the system did.
async fn assert_finished(fx: &AppFx, live: &Live, branch: &str) {
    let (status, v) = fx.finish(branch).await;
    assert_eq!(status, 200, "{v}");
    let b = records(&fx.state.store).branch(branch).await.unwrap();
    let scored = matches!(b.status, BranchStatus::Passed | BranchStatus::Failed);
    assert!(scored, "{:?}", b.status);
    assert_eq!(v["status"], json!(b.status));
    let evs = fx.events(branch).await;
    live::assert_recorded(&b, &evs, live);
    assert_eq!(evs[0].payload["runner"], "external");
    let k = kinds(&evs);
    assert_eq!(k[..2], [EventKind::RunStart, EventKind::AgentStart]);
    assert_eq!(k.last(), Some(&EventKind::Check));
    let ids: Vec<_> = b.team.agents.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["solo", "reviewer"]);
    assert!(b.team.agents.iter().all(|a| a.kind == AgentKind::External));
}

/// The agent's model calls as its client reported them: one per message, each with its usage,
/// and the metrics' count and peak context are theirs.
async fn assert_reported(fx: &AppFx, branch: &str, ran: &live::ExternalRun) {
    let evs = fx.events(branch).await;
    let calls: Vec<&Event> = evs
        .iter()
        .filter(|e| e.kind == EventKind::ModelCall)
        .collect();
    assert_eq!(calls.len(), ran.reports.len());
    assert!(
        !calls.is_empty(),
        "Claude Code made at least one model call"
    );
    for (c, r) in calls.iter().zip(&ran.reports) {
        assert_eq!(c.payload["reported_by"], "client");
        assert_eq!(c.payload["message_id"], r["message_id"]);
        assert_eq!(c.payload["usage"]["prompt_tokens"], whole_input(r));
    }
    let peak = calls
        .iter()
        .map(|c| c.payload["usage"]["prompt_tokens"].as_i64().unwrap())
        .max();
    let a = fx.metrics_of(branch).await["agents"][0].clone();
    assert_eq!(a["model_calls"], json!(calls.len()));
    assert_eq!(a["peak_prompt_tokens"], json!(peak.unwrap()));
}

/// An external system drives a run with Claude Code as its agent: Arena records the run's start,
/// the agent's start, each tool call it made through its endpoint (with its fields), each model
/// call its client reported, its end, and the check when the run is finished. A read-only agent
/// that joins later is offered no write tool.
#[test]
#[ignore = "live: runs Claude Code (support/live.rs)"]
fn live_an_external_system_drives_a_run_and_arena_records_it() {
    run_live(
        Live::claude("live_an_external_system_drives_a_run_and_arena_records_it"),
        an_external_system_drives_a_run_and_arena_records_it,
    );
}

async fn an_external_system_drives_a_run_and_arena_records_it(live: Live) {
    let fx = setup("drive").await;
    let branch = start(&fx).await;
    let task = ticket(&fx).await;
    let (reg, ran) = work(
        &fx,
        &live,
        &branch,
        json!({"id": "solo", "model": live.model()}),
        &task,
    )
    .await;
    let tools = reg["tools"].to_string();
    assert!(tools.contains("exchange_delivered_order_items") && tools.contains("\"inbox\""));
    assert_reviewer_reads_only(&fx, &branch).await;
    assert_reported(&fx, &branch, &ran).await;
    assert_finished(&fx, &live, &branch).await;
    assert_eq!(
        fx.finish(&branch).await.0,
        404,
        "a finished branch is no longer live"
    );
}

/// A fork of an external run hands the system each agent's history up to the fork point and its
/// endpoints on the new branch, whose world kept what was done before it; a fork may give
/// another team instead (not with agent changes too), whose agents have no history.
#[test]
#[ignore = "live: runs Claude Code (support/live.rs)"]
fn live_a_fork_of_an_external_run_hands_back_each_agents_history() {
    run_live(
        Live::claude("live_a_fork_of_an_external_run_hands_back_each_agents_history"),
        a_fork_of_an_external_run_hands_back_each_agents_history,
    );
}

async fn a_fork_of_an_external_run_hands_back_each_agents_history(live: Live) {
    let fx = setup("fork").await;
    let branch = start(&fx).await;
    let task = ticket(&fx).await;
    work(
        &fx,
        &live,
        &branch,
        json!({"id": "solo", "model": live.model()}),
        &task,
    )
    .await;
    register(&fx, &branch, json!({"id": "reviewer", "read_only": true})).await;
    assert_eq!(fx.finish(&branch).await.0, 200);
    let evs = fx.events(&branch).await;
    // Fork at the reviewer's start: after everything the first agent did.
    let at = live::first_of(&evs, EventKind::AgentStart, "reviewer").unwrap();
    assert_history_handed_back(&fx, &branch, &evs, at).await;
    assert_team_swap(&fx, &branch, at).await;
}

/// A fork at `at` hands back each agent's history before it and its endpoints on the new branch.
async fn assert_history_handed_back(fx: &AppFx, branch: &str, evs: &[Event], at: i64) {
    let (status, f) = fx.on(branch, "fork", json!({"at_seq": at})).await;
    assert_eq!((status, &f["start_seq"]), (200, &json!(at)), "{f}");
    let agents = f["agents"].as_array().unwrap();
    let of = |id: &str| agents.iter().find(|a| a["agent"] == id).unwrap().clone();
    let history = of("solo")["history"].as_array().unwrap().clone();
    let want: Vec<Value> = evs
        .iter()
        .filter(|e| e.seq < at && e.agent.as_deref() == Some("solo"))
        .map(|e| json!(e.kind))
        .collect();
    let got: Vec<Value> = history.iter().map(|e| e["kind"].clone()).collect();
    assert_eq!(got, want);
    let child = f["branch_id"].as_str().unwrap().to_string();
    assert!(fx.state.live.contains(&child));
    assert!(of("reviewer")["mcp_url"].as_str().unwrap().contains(&child));
    let (status, v) = fx.finish(&child).await;
    assert_eq!(status, 200, "{v}");
}

/// A fork may give another team instead of agent changes (not both); its agents have no history.
async fn assert_team_swap(fx: &AppFx, branch: &str, at: i64) {
    let swap_team = json!({"id": "single", "name": "Single agent", "agents": [
        {"id": "solo2", "name": "Solo", "kind": "external", "model": null, "prompt": "You do it all."}]});
    let both = json!({"changes": {"solo": {"prompt": "x"}}, "at_seq": at, "team": swap_team});
    assert_eq!(fx.on(branch, "fork", both).await.0, 400);
    let (status, f) = fx
        .on(branch, "fork", json!({"at_seq": at, "team": swap_team}))
        .await;
    assert_eq!(status, 200, "{f}");
    let agents = f["agents"].as_array().unwrap();
    assert_eq!((agents.len(), &agents[0]["agent"]), (1, &json!("solo2")));
    assert_eq!(agents[0]["history"], json!([]));
    let swapped = f["branch_id"].as_str().unwrap();
    let stored = records(&fx.state.store).branch(swapped).await.unwrap();
    assert_eq!(stored.team.id, "single");
    assert!(stored.fork_note.unwrap().contains("Single agent"));
    assert_eq!(fx.finish(swapped).await.0, 200);
}

// ---------------------------------------------------------------- run limits: window and spend

/// Starts an external run of task 27 with `extra` fields (`world_minutes`, `max_cost_usd`): its
/// branch id.
async fn start_with(fx: &AppFx, extra: Value) -> String {
    let mut body = json!({"task_id": "27", "team": team(json!([])), "runner": "external"});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let (status, v) = fx.post("/api/runs", body).await;
    assert_eq!(status, 200, "{v}");
    v["branch_id"].as_str().unwrap().to_string()
}

async fn status_of(fx: &AppFx, branch: &str) -> Value {
    let (_, b) = send(fx.http.get(format!("{}/api/branches/{branch}", fx.base))).await;
    b
}

/// Waits (at most `secs`) until the branch is no longer running: its record. Scoring a run of
/// task 27 first builds the task's gold save on the test's own hub, which can take seconds of its
/// own beside other tests, so the waits leave room past the window.
async fn ended_within(fx: &AppFx, branch: &str, secs: u64) -> Value {
    let since = std::time::Instant::now();
    loop {
        let b = status_of(fx, branch).await;
        if b["status"] != "running" {
            return b;
        }
        if since.elapsed().as_secs() >= secs {
            let evs = fx.events(branch).await;
            let said: Vec<String> = (evs.iter())
                .map(|e| format!("{:?} {} {}", e.kind, e.payload["part"], e.payload["kind"]))
                .collect();
            panic!("still running after {secs} s: {said:?}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// The payload of the branch's world event `part`/`kind`, if it has one.
async fn world_event(fx: &AppFx, branch: &str, part: &str, kind: &str) -> Option<Value> {
    let evs = fx.events(branch).await;
    let hit = |e: &&Event| {
        e.kind == EventKind::WorldEvent && e.payload["part"] == part && e.payload["kind"] == kind
    };
    evs.iter().find(hit).map(|e| e.payload.clone())
}

/// The branch was scored after its window's end at clock `at_least` or later.
async fn scored_at_the_window(fx: &AppFx, branch: &str, at_least: i64) {
    let evs = fx.events(branch).await;
    let up = |e: &&Event| e.kind == EventKind::WorldEvent && e.payload["kind"] == "time_up";
    let up = evs.iter().find(up).expect("a time_up event");
    let check = evs
        .iter()
        .find(|e| e.kind == EventKind::Check)
        .expect("the run was scored");
    assert!(
        up.payload["clock_ms"].as_i64().unwrap() >= at_least,
        "{}",
        up.payload
    );
    assert!(check.seq > up.seq, "scored before the window ended");
}

#[test]
fn a_world_time_window_ends_the_run_scored_at_its_limit() {
    run(async {
        let fx = setup("window").await;
        let branch = start_with(&fx, json!({"world_minutes": 0.05})).await;
        ended_within(&fx, &branch, 30).await;
        scored_at_the_window(&fx, &branch, 3000).await;
        let up = world_event(&fx, &branch, "window", "time_up")
            .await
            .expect("a time_up event");
        assert!(up["clock_ms"].as_i64().unwrap() >= 3000, "{up}");
        assert_eq!(up["text"], "the run's world time window (0.05 min) ended");
    })
}

#[test]
fn a_driver_that_finishes_early_is_scored_at_the_limit() {
    run(async {
        let fx = setup("window-early").await;
        let branch = start_with(&fx, json!({"world_minutes": 0.1})).await;
        register(&fx, &branch, json!({"id": "a"})).await;
        let (status, v) = fx.finish(&branch).await;
        assert_eq!(
            (status, &v),
            (
                200,
                &json!({"status": "running", "held_until_clock_ms": 6000})
            )
        );
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        assert_eq!(status_of(&fx, &branch).await["status"], "running");
        ended_within(&fx, &branch, 30).await;
        scored_at_the_window(&fx, &branch, 6000).await;
    })
}

#[test]
fn a_held_finish_with_an_error_ends_failed_and_scored() {
    run(async {
        let fx = setup("window-error").await;
        let branch = start_with(&fx, json!({"world_minutes": 0.05})).await;
        let (status, v) = fx
            .finish_with(&branch, json!({"error": "driver raised X"}))
            .await;
        assert_eq!((status, &v["status"]), (200, &json!("running")), "{v}");
        let b = ended_within(&fx, &branch, 30).await;
        assert_eq!(
            (&b["status"], &b["error"]),
            (&json!("failed"), &json!("driver raised X"))
        );
        assert!(kinds(&fx.events(&branch).await).contains(&EventKind::Check));
    })
}

#[test]
fn a_pause_holds_the_window() {
    run(async {
        let fx = setup("window-pause").await;
        let since = std::time::Instant::now();
        let branch = start_with(&fx, json!({"world_minutes": 0.05})).await;
        assert_eq!(fx.on(&branch, "pause", json!({})).await.0, 200);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        assert_eq!(fx.on(&branch, "resume", json!({})).await.0, 200);
        ended_within(&fx, &branch, 30).await;
        assert!(since.elapsed().as_millis() >= 5000, "{:?}", since.elapsed());
    })
}

#[test]
fn world_minutes_are_checked_and_recorded() {
    run(async {
        let fx = setup("window-bad").await;
        for bad in [json!(0), json!(-1), json!(1441)] {
            let body = json!({"task_id": "27", "team": team(json!([])), "runner": "external", "world_minutes": bad});
            let (status, v) = fx.post("/api/runs", body).await;
            assert_eq!(status, 400, "{v}");
            assert_eq!(
                v["error"],
                "world_minutes must be from 0 to 1440 minutes of world time"
            );
        }
        let branch = start_with(&fx, json!({"world_minutes": 60})).await;
        let evs = fx.events(&branch).await;
        let start = evs.iter().find(|e| e.kind == EventKind::RunStart).unwrap();
        assert_eq!(start.payload["world_minutes"], json!(60.0));
        assert_eq!(fx.on(&branch, "stop", json!({})).await.0, 200);
    })
}

#[test]
fn runs_without_a_window_finish_as_before() {
    run(async {
        let fx = setup("no-window").await;
        let branch = start(&fx).await;
        let (status, v) = fx.finish(&branch).await;
        assert_eq!(status, 200, "{v}");
        assert_ne!(v["status"], "running", "{v}");
        assert!(v.get("held_until_clock_ms").is_none(), "{v}");
    })
}

/// An app whose one provider, `local:pod`, is the fake server at `base` with a high rate.
async fn rated_pod(tag: &str, base: &str) -> AppFx {
    use shaked::providers::{Endpoint, LocalEndpoint, ProviderConfig, Rate};
    let mut providers = ProviderConfig::default();
    let rate = Some(Rate {
        input_per_mtok: 1000.0,
        output_per_mtok: 1000.0,
    });
    let endpoint = Endpoint::new(base, None);
    let pod = LocalEndpoint {
        name: "pod".into(),
        endpoint,
        rate,
    };
    providers.locals.push(pod);
    AppFx::idle("external", tag)
        .providers(providers)
        .start()
        .await
}

/// The statuses of `n` model calls through the gateway `gateway_url`.
async fn calls(fx: &AppFx, reg: &Value, n: usize) -> Vec<u16> {
    let url = format!("{}/chat/completions", reg["gateway_url"].as_str().unwrap());
    let token = reg["token"].as_str().unwrap();
    let call = json!({"model": "m", "messages": [{"role": "user", "content": "go"}]});
    let mut statuses = vec![];
    for _ in 0..n {
        let r = fx
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&call)
            .send()
            .await
            .unwrap();
        statuses.push(r.status().as_u16());
    }
    statuses
}

#[test]
fn a_run_over_its_spend_limit_is_refused_further_model_calls() {
    use support::fake_llm::{fake_llm, gpt_oss};
    run(async {
        let llm = fake_llm(vec![(200, gpt_oss("t", None))]).await;
        let fx = rated_pod("spend", &llm.base).await;
        let branch = start_with(&fx, json!({"max_cost_usd": 0.01})).await;
        let join = json!({"id": "a", "provider": "local:pod", "model": "m"});
        let reg = register(&fx, &branch, join).await;
        let statuses = calls(&fx, &reg, 3).await;
        assert_eq!(statuses, [200, 402, 402]);
        assert_eq!(llm.seen.lock().unwrap().len(), 1);
        let evs = fx.events(&branch).await;
        let reached =
            |e: &&Event| e.kind == EventKind::WorldEvent && e.payload["kind"] == "limit_reached";
        assert_eq!(evs.iter().filter(reached).count(), 1);
        assert!(world_event(&fx, &branch, "budget", "limit_reached")
            .await
            .is_some());
        assert_eq!(fx.finish(&branch).await.0, 200);
    })
}
