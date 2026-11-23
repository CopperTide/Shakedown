//! The dispatch runner: a lead delegates, its team works in parallel, reports back and stays
//! available, and the lead finishes. Without a model: what the API refuses, the teams it
//! suggests, and a lead that can't reach its model. `live_`: a team on a real model (see
//! `support/live.rs`), then a fork of its run.
mod support;
use support::of_kind;
use support::records;
use support::team_of;

use std::collections::HashMap;

use serde_json::json;
use shaked::{fork, model::*, providers::ProviderConfig, runner};
use support::fake_apis::test_vault;
use support::fake_llm::{fake_llm, gpt_oss, FakeLlm, Seen};
use support::{
    app, app::send, keys, live, live::Live, poll_until, run, written, AppFx, SAVES_BUILT, STEP,
};

/// An app on the retail benchmark with the real drivers; `providers` replaces the environment's.
async fn setup(tag: &str, providers: Option<ProviderConfig>) -> AppFx {
    let app = AppFx::configured("dispatch", tag).bench("retail");
    match providers {
        Some(p) => app.providers(p),
        None => app,
    }
    .start()
    .await
}

fn spec(id: &str, kind: AgentKind) -> AgentSpec {
    AgentSpec::new(id, kind)
        .with_model("m")
        .with_prompt(format!("You are {id}."))
}

/// A Codex agent can't keep a session, and a team has one lead: both refused before anything
/// runs. The teams endpoint gives each suggested team with its runner.
#[test]
fn the_api_refuses_what_a_dispatch_run_cant_take_and_lists_the_teams() {
    run(async {
        let fx = setup("api", None).await;
        let http = reqwest::Client::new();
        let post = |t: Team| {
            let body = json!({"task_id": "27", "team": t, "runner": "dispatch"});
            send(http.post(format!("{}/api/runs", fx.base)).json(&body))
        };
        let codex = team_of(vec![
            spec("lead", AgentKind::ClaudeCode),
            spec("x", AgentKind::Codex),
        ]);
        let (status, body) = post(codex).await;
        assert_eq!(status, 400, "{body}");
        assert!(body["error"].as_str().unwrap().contains("Codex"), "{body}");
        let mut two = team_of(vec![
            spec("a", AgentKind::ClaudeCode),
            spec("b", AgentKind::ClaudeCode),
        ]);
        two.agents.iter_mut().for_each(|a| a.lead = true);
        let (status, body) = post(two).await;
        assert_eq!(status, 400, "{body}");
        assert!(records(&fx.state.store).runs().await.unwrap().is_empty());
        let (status, teams) = send(http.get(format!("{}/api/teams?model=haiku", fx.base))).await;
        assert_eq!(status, 200);
        let first = &teams[0];
        assert_eq!(first["runner"], "chain");
        assert!(first["agents"][0]["model"] == "haiku", "{first}");
    })
}

/// The lead's first turn fails when its provider can't be reached: the run ends `infra_error`,
/// with the lead's wake and idle recorded and every agent's start and end.
#[test]
fn a_lead_that_cant_reach_its_model_ends_the_run_infra_error() {
    run(async {
        let providers = ProviderConfig::with_upstream(app::NOWHERE);
        let fx = setup("infra", Some(providers)).await;
        let t = team_of(vec![
            spec("lead", AgentKind::Open),
            spec("helper", AgentKind::Open),
        ]);
        let started = runner::RunOptions::new(Runner::Dispatch).start(&fx.state, "27", t);
        let (_run, branch) = started.await.unwrap();
        let b = live::ended(&fx.state, &branch).await;
        assert_eq!(b.status, BranchStatus::InfraError, "{:?}", b.error);
        let error = b.error.unwrap();
        assert!(
            error.starts_with("the lead failed: model call failed (502)"),
            "{error}"
        );
        let evs = records(&fx.state.store).events(&branch).await.unwrap();
        assert_eq!(evs[0].payload["runner"], "dispatch");
        assert_lifecycle_events(&evs);
        assert_lead_woken_with_its_brief(&evs);
        assert_dispatch_payloads(&evs);
        assert!(fx.state.live.is_empty());
    })
}

fn assert_lifecycle_events(evs: &[Event]) {
    let kinds: Vec<(EventKind, Option<&str>)> = (evs.iter())
        .filter(|e| {
            !matches!(
                e.kind,
                EventKind::ToolCall | EventKind::ModelCall | EventKind::WorldView
            )
        })
        .map(|e| (e.kind, e.agent.as_deref()))
        .collect();
    let lead = Some("lead");
    let helper = Some("helper");
    assert_eq!(
        kinds,
        [
            (EventKind::RunStart, None),
            (EventKind::AgentStart, lead),
            (EventKind::AgentStart, helper),
            (EventKind::Wake, lead),
            (EventKind::Idle, lead),
            (EventKind::AgentEnd, lead),
            (EventKind::AgentEnd, helper),
        ]
    );
}

/// What a dispatch run records when it starts its agents, wakes the lead and the lead goes idle
/// (golden).
fn assert_dispatch_payloads(evs: &[Event]) {
    let starts: Vec<serde_json::Value> = (of_kind(evs, EventKind::AgentStart).iter())
        .map(|e| written(&e.payload))
        .collect();
    let start = |id: &str| json!({"name": id, "kind": "open", "model": "m"});
    assert_eq!(starts, [start("lead"), start("helper")]);
    let wake = &of_kind(evs, EventKind::Wake)[0].payload;
    assert_eq!(keys(wake), ["input", "mail", "reason"]);
    assert_eq!(wake["mail"], json!([]));
    let idle = &of_kind(evs, EventKind::Idle)[0].payload;
    assert_eq!(keys(idle), ["error", "ok", "text"]);
    assert_eq!((&idle["ok"], &idle["text"]), (&json!(false), &json!("")));
    assert!(idle["error"].is_string(), "{idle}");
}

fn assert_lead_woken_with_its_brief(evs: &[Event]) {
    let wake = &of_kind(evs, EventKind::Wake)[0].payload;
    assert_eq!(wake["reason"], "start");
    let input = wake["input"].as_str().unwrap();
    assert!(
        input.contains("You lead this run") && input.contains("`helper`"),
        "{input}"
    );
}

#[test]
#[ignore = "live: runs a real model (support/live.rs)"]
fn live_a_lead_dispatches_its_team_works_in_parallel_reports_and_the_lead_finishes() {
    if let Some(l) = Live::any("live_a_lead_dispatches_its_team") {
        run(a_lead_dispatches(l));
    }
}

const LEAD: &str = "You lead a two-agent check of the retail store. In your first turn, call \
`delegate` twice, one right after the other: to `orders` with the task \"Look up the order of the \
ticket with one read tool and report its status in one line\", and to `customer` with the task \"Look \
up the customer of the ticket with one read tool and report their name in one line\". Then end your \
turn. When both have reported, call `finish` with a one-line summary. Change nothing in the world.";

const MEMBER: &str = "You are one agent of a team. Do exactly the task you are given with read \
tools only, then call `report` once with one line. Change nothing in the world.";

async fn a_lead_dispatches(live: Live) {
    let fx = setup("live", None).await;
    let mut lead = live.agent("lead", LEAD);
    lead.lead = true;
    let t = live.team(vec![
        live.agent("orders", MEMBER),
        lead,
        live.agent("customer", MEMBER),
    ]);
    let started = runner::RunOptions::new(Runner::Dispatch).start(&fx.state, "27", t);
    let (_run, branch) = started.await.unwrap();
    let (b, evs) = live::ended_ok(&fx.state, &branch).await;
    live::assert_recorded(&b, &evs, &live);
    assert_eq!(evs[0].payload["runner"], "dispatch");
    assert_dispatched(&evs);
    assert_parallel(&evs);
    assert_finished_last(&evs);
    if matches!(live, Live::Claude { .. }) {
        assert_reasoning_from_the_stream(&evs);
    }
    fork_restarts_everyone(&fx, &branch, &evs).await;
}

/// Claude Code's sessions: what the agents said between tool calls was recorded as their reasoning,
/// each tied to a model call recorded before it.
fn assert_reasoning_from_the_stream(evs: &[Event]) {
    let said = of_kind(evs, EventKind::Reasoning);
    let steps: Vec<_> = said
        .iter()
        .filter(|e| e.payload["source"] == "claude_code" && e.payload["kind"] == "step")
        .collect();
    assert!(!steps.is_empty(), "no reasoning from Claude Code's stream");
    for e in steps {
        let Some(seq) = e.payload["model_call_seq"].as_i64() else {
            continue;
        };
        let call = evs.iter().find(|c| c.seq == seq).unwrap();
        assert_eq!((call.kind, &call.agent), (EventKind::ModelCall, &e.agent));
    }
}

/// Each delegation woke its agent with it, and each agent that was given work reported to the
/// lead (itself, or shakedown passing on its turn's last words).
fn assert_dispatched(evs: &[Event]) {
    let delegations = of_kind(evs, EventKind::Delegation);
    assert!(!delegations.is_empty(), "the lead delegated");
    for d in &delegations {
        assert_eq!(d.agent.as_deref(), Some("lead"));
        let to = d.payload["to"].as_str().unwrap();
        let woke = evs.iter().any(|e| {
            e.kind == EventKind::Wake
                && e.agent.as_deref() == Some(to)
                && e.payload["mail"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(d.seq))
        });
        assert!(woke, "delegation {} woke {to}", d.seq);
        let reported = evs.iter().any(|e| {
            e.seq > d.seq && e.kind == EventKind::Report && e.agent.as_deref() == Some(to)
        });
        assert!(reported, "{to} reported after delegation {}", d.seq);
    }
    let reports = of_kind(evs, EventKind::Report);
    assert!(reports.iter().all(|r| r.payload["to"] == "lead"));
}

/// The working spans (wake to idle) of each agent.
fn spans(evs: &[Event]) -> HashMap<String, Vec<(i64, i64)>> {
    let mut open: HashMap<String, i64> = HashMap::new();
    let mut out: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    for e in evs {
        let Some(a) = e.agent.clone() else { continue };
        match e.kind {
            EventKind::Wake => {
                open.insert(a, e.seq);
            }
            EventKind::Idle | EventKind::AgentEnd => {
                if let Some(s) = open.remove(&a) {
                    out.entry(a).or_default().push((s, e.seq));
                }
            }
            _ => {}
        }
    }
    out
}

/// The two specialists, delegated to in the lead's same turn, worked at the same time.
fn assert_parallel(evs: &[Event]) {
    let s = spans(evs);
    let (Some(o), Some(c)) = (s.get("orders"), s.get("customer")) else {
        panic!("both specialists worked: {s:?}");
    };
    let overlap = o.iter().any(|a| c.iter().any(|b| a.0 < b.1 && b.0 < a.1));
    assert!(overlap, "orders {o:?} and customer {c:?} worked at once");
}

/// The lead's finish is the run's last word: after it only agents' ends (and turns ending),
/// then the check; no tool call lands after it (its closing model call and words may).
fn assert_finished_last(evs: &[Event]) {
    let finish = of_kind(evs, EventKind::Finish);
    assert_eq!(finish.len(), 1, "the lead finished once");
    assert_eq!(finish[0].agent.as_deref(), Some("lead"));
    let after = evs.iter().filter(|e| e.seq > finish[0].seq);
    for e in after {
        let ok = matches!(
            e.kind,
            EventKind::AgentEnd
                | EventKind::Idle
                | EventKind::Check
                | EventKind::ModelCall
                | EventKind::Reasoning
        );
        assert!(ok, "{:?} after the finish: {}", e.kind, e.payload);
    }
    assert_eq!(evs.last().unwrap().kind, EventKind::Check);
}

/// A fork at the lead's second delegation restarts every agent there: the lead, working at the
/// fork point, carries on with its history; the run goes on to its end.
async fn fork_restarts_everyone(fx: &AppFx, branch: &str, evs: &[Event]) {
    let delegations = of_kind(evs, EventKind::Delegation);
    let at = delegations.get(1).unwrap_or(&delegations[0]).seq;
    let forked = fork::fork(&fx.state, branch, at, HashMap::new())
        .await
        .unwrap();
    assert_eq!(forked.start_seq, at, "a dispatch run forks where asked");
    let b = live::ended(&fx.state, &forked.branch_id).await;
    assert_ne!(b.status, BranchStatus::InfraError, "{:?}", b.error);
    let evs = records(&fx.state.store)
        .events(&forked.branch_id)
        .await
        .unwrap();
    let new: Vec<&Event> = evs.iter().filter(|e| e.seq >= at).collect();
    let lead_wake = new
        .iter()
        .find(|e| e.kind == EventKind::Wake && e.agent.as_deref() == Some("lead"));
    let wake = lead_wake.expect("the lead was woken on the fork");
    assert_eq!(wake.payload["reason"], "resume");
    let input = wake.payload["input"].as_str().unwrap();
    assert!(input.contains("Your work so far"), "{input}");
    if delegations.len() > 1 {
        assert!(
            input.contains("You gave `"),
            "the first delegation is in its history: {input}"
        );
    }
    let ends = new.iter().filter(|e| e.kind == EventKind::AgentEnd).count();
    assert_eq!(ends, 3, "every agent ended on the fork");
}

/// A dispatch agent may name one of your stored keys (`key:<name>`): its calls go out with that
/// key, as a chain run's do.
#[test]
fn a_dispatch_agent_runs_on_the_stored_key_it_names() {
    run(async {
        let llm = fake_llm(vec![(200, gpt_oss("t", Some("done")))]).await;
        let fx = setup("keyed", Some(ProviderConfig::with_upstream(app::NOWHERE))).await;
        let _ = fx.state.vault.set(test_vault());
        let key = json!({"name": "mine", "kind": "openai", "secret": "k-own-7",
                         "base_url": llm.base});
        let (status, v) = fx.post("/api/models/keys", key).await;
        assert_eq!(status, 200, "{v}");
        let lead = spec("lead", AgentKind::Open).with_provider("key:mine");
        let t = team_of(vec![lead, spec("helper", AgentKind::Open)]);
        let started = runner::RunOptions::new(Runner::Dispatch).start(&fx.state, "27", t);
        let (_run, branch) = started.await.unwrap();
        let asked = async || asked_or_ended(&fx, &llm, &branch).await;
        let seen = poll_until(SAVES_BUILT, STEP, asked).await;
        fx.post(&format!("/api/branches/{branch}/stop"), json!({}))
            .await;
        let b = live::ended(&fx.state, &branch).await;
        let seen = seen.flatten();
        let auth = seen.unwrap_or_else(|| panic!("no call went out: {:?}", b.error));
        assert_eq!(auth.authorization.as_deref(), Some("Bearer k-own-7"));
    })
}

/// The first call `llm` got (`Some(Some(_))`), else `Some(None)` once `branch` ended without
/// one; `None` while neither.
async fn asked_or_ended(fx: &AppFx, llm: &FakeLlm, branch: &str) -> Option<Option<Seen>> {
    let first = llm.seen.lock().unwrap().first().cloned();
    if first.is_some() {
        return Some(first);
    }
    let b = records(&fx.state.store).find_branch(branch).await.ok()??;
    (b.status != BranchStatus::Running).then_some(None)
}
