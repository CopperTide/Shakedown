use std::collections::HashMap;

use serde_json::{json, Value};

use super::{plan::is_change, start::new_team, *};
use crate::{model::*, providers::ProviderConfig, world::Saves};

fn spec(id: &str, kind: AgentKind) -> AgentSpec {
    AgentSpec::new(id, kind).with_prompt(format!("You are {id}."))
}

fn team(resolver: AgentKind) -> Team {
    Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![spec("triage", AgentKind::Open), spec("resolver", resolver)],
    }
}

fn ev(seq: i64, agent: Option<&str>, kind: EventKind, payload: Value) -> Event {
    crate::testkit::event(seq, agent, kind, payload)
}

fn msgs(tag: &str) -> Vec<Value> {
    vec![
        json!({"role": "system", "content": "sys"}),
        json!({"role": "user", "content": tag}),
    ]
}

fn mc(seq: i64, agent: &str, tag: &str) -> Event {
    ev(
        seq,
        Some(agent),
        EventKind::ModelCall,
        json!({"request": {"messages": msgs(tag)}, "response": {}, "usage": {}}),
    )
}

fn read(seq: i64, agent: &str) -> Event {
    let p = json!({"name": "get_order_details", "args": {}, "result": {}, "error": null,
                   "reads": ["orders/#W1"], "writes": [], "snapshot": null, "tool_call_id": "x"});
    ev(seq, Some(agent), EventKind::ToolCall, p)
}

fn write(seq: i64, agent: &str, snapshot: Option<&str>) -> Event {
    let mut p = json!({"name": "exchange_delivered_order_items", "args": {}, "result": {}, "error": null,
                       "reads": ["orders/#W1"], "writes": ["orders/#W1"], "snapshot": snapshot, "tool_call_id": "x"});
    if snapshot.is_none() {
        p["snapshot_error"] = json!("disk full");
    }
    ev(seq, Some(agent), EventKind::ToolCall, p)
}

fn start(seq: i64, agent: &str) -> Event {
    ev(seq, Some(agent), EventKind::AgentStart, json!({}))
}

/// 1 RunStart; triage 2..=7 (handoff at 6); resolver 8..=14 with writes snapshotted at 10
/// (snap_a) and 12 (snap_b); 15 Check.
fn events() -> Vec<Event> {
    vec![
        ev(1, None, EventKind::RunStart, json!({"task_id": "27"})),
        start(2, "triage"),
        mc(3, "triage", "m3"),
        read(4, "triage"),
        mc(5, "triage", "m5"),
        ev(6, Some("triage"), EventKind::Handoff, json!({"note": "N1"})),
        ev(7, Some("triage"), EventKind::AgentEnd, json!({"ok": true})),
        start(8, "resolver"),
        mc(9, "resolver", "m9"),
        write(10, "resolver", Some("snap_a")),
        mc(11, "resolver", "m11"),
        write(12, "resolver", Some("snap_b")),
        mc(13, "resolver", "m13"),
        ev(
            14,
            Some("resolver"),
            EventKind::AgentEnd,
            json!({"ok": true}),
        ),
        ev(15, None, EventKind::Check, json!({"passed": false})),
    ]
}

fn base() -> Saves {
    [("retail".to_string(), "world_base".to_string())].into()
}

fn snap(s: &str) -> Saves {
    [("retail".to_string(), s.to_string())].into()
}

fn no_changes() -> HashMap<String, Change> {
    HashMap::new()
}

fn change(agent: &str, c: Change) -> HashMap<String, Change> {
    [(agent.to_string(), c)].into_iter().collect()
}

/// `events()` as a run of today recorded it, with `runner`.
fn events_of(runner: &str) -> Vec<Event> {
    let mut evs = events();
    evs[0].payload = json!({"task_id": "27", "runner": runner, "handoff": "optional"});
    evs
}

#[test]
fn only_an_external_run_takes_another_team_and_not_with_changes() {
    let start = |runner: &str| vec![ev(1, None, EventKind::RunStart, json!({"runner": runner}))];
    let id = "single".to_string();
    let mut req = ForkRequest {
        at_seq: 1,
        changes: HashMap::new(),
        restore: parts::proto::Mode::Exact,
        team: Some(Team {
            id,
            ..team(AgentKind::Open)
        }),
    };
    let parent = team(AgentKind::Open);
    let refused = new_team(&parent, &start("chain"), &req);
    assert!(matches!(
        refused,
        Err(ForkError::TeamChange(TEAM_NOT_EXTERNAL))
    ));
    let swapped = new_team(&parent, &start("external"), &req).unwrap();
    assert_eq!(swapped.id, "single");
    let prompt = Some("x".to_string());
    req.changes.insert(
        "triage".into(),
        Change {
            prompt,
            ..Change::default()
        },
    );
    let both = new_team(&parent, &start("external"), &req);
    assert!(matches!(both, Err(ForkError::TeamChange(TEAM_AND_CHANGES))));
}

#[test]
fn a_concurrent_run_restarts_every_agent() {
    let p = plan_fork(
        &events_of("concurrent"),
        &team(AgentKind::Open),
        12,
        &no_changes(),
    );
    let p = p.unwrap();
    assert_eq!((p.start_seq, p.agent_index), (2, 0));
    assert_eq!(p.reason.as_deref(), Some(REASON_CONCURRENT));
    assert_eq!((p.restore, p.resume, p.notes), (base(), None, vec![]));
}

#[test]
fn a_new_chain_needs_no_handoff_to_fork_after_it() {
    let mut evs = events_of("chain");
    evs.retain(|e| e.kind != EventKind::Handoff);
    let p = plan_fork(&evs, &team(AgentKind::Open), 11, &no_changes()).unwrap();
    assert_eq!((p.start_seq, p.agent_index, p.notes), (11, 1, vec![]));
    // A legacy run without the handoff can't be forked there.
    let mut legacy = events();
    legacy.retain(|e| e.kind != EventKind::Handoff);
    let refused = plan_fork(&legacy, &team(AgentKind::Open), 11, &no_changes());
    assert!(matches!(refused, Err(ForkError::MissingHandoff(_))));
}

#[test]
fn cli_agent_moves_to_agent_start() {
    let p = plan_fork(&events(), &team(AgentKind::ClaudeCode), 12, &no_changes()).unwrap();
    assert_eq!(p.start_seq, 8);
    assert_eq!(p.moved_from, Some(12));
    assert_eq!(
        p.reason.as_deref(),
        Some("Claude Code and Codex sessions restart from the agent's start")
    );
    assert_eq!(p.agent_index, 1);
    assert_eq!(p.resume, None);
    assert_eq!(p.restore, base());
    assert_eq!(p.notes, vec!["N1".to_string()]);
}

#[test]
fn open_tool_call_moves_to_its_model_call() {
    let p = plan_fork(&events(), &team(AgentKind::Open), 12, &no_changes()).unwrap();
    assert_eq!(p.start_seq, 11);
    assert_eq!(p.moved_from, Some(12));
    assert_eq!(
        p.reason.as_deref(),
        Some("a tool call is redone from the model call that chose it")
    );
    assert_eq!(p.agent_index, 1);
    assert_eq!(p.resume, Some(msgs("m11")));
    assert_eq!(p.restore, snap("snap_a"));
    assert_eq!(p.notes, vec!["N1".to_string()]);
}

#[test]
fn open_model_call_stays_and_resumes_messages() {
    let p = plan_fork(&events(), &team(AgentKind::Open), 11, &no_changes()).unwrap();
    assert_eq!(p.start_seq, 11);
    assert_eq!(p.moved_from, None);
    assert_eq!(p.reason, None);
    assert_eq!(p.agent_index, 1);
    assert_eq!(p.resume, Some(msgs("m11")));
    assert_eq!(p.restore, snap("snap_a"));
    // A prompt change to the active agent doesn't move the fork point.
    let c = change(
        "resolver",
        Change {
            prompt: Some("New.".into()),
            ..Default::default()
        },
    );
    let p = plan_fork(&events(), &team(AgentKind::Open), 11, &c).unwrap();
    assert_eq!((p.start_seq, p.moved_from), (11, None));
}

#[test]
fn change_to_finished_agent_moves_back() {
    let c = change(
        "triage",
        Change {
            prompt: Some("Be brief.".into()),
            ..Default::default()
        },
    );
    let p = plan_fork(&events(), &team(AgentKind::Open), 12, &c).unwrap();
    assert_eq!(p.start_seq, 2);
    assert_eq!(p.moved_from, Some(12));
    assert_eq!(
        p.reason.as_deref(),
        Some("a change targets an agent that finished earlier")
    );
    assert_eq!(p.agent_index, 0);
    assert_eq!(p.resume, None);
    assert_eq!(p.restore, base());
    assert!(p.notes.is_empty());
}

fn provider(agent: &str, p: &str) -> HashMap<String, Change> {
    change(
        agent,
        Change {
            provider: Some(p.into()),
            ..Default::default()
        },
    )
}

#[test]
fn provider_change_is_a_model_change() {
    let t = team(AgentKind::Open);
    // Like a model change, it resumes the active open agent from the model call ...
    let p = plan_fork(&events(), &t, 11, &provider("resolver", "local:box")).unwrap();
    assert_eq!((p.start_seq, p.moved_from, p.agent_index), (11, None, 1));
    assert_eq!(p.resume, Some(msgs("m11")));
    // ... and moves back to a finished agent it targets.
    let p = plan_fork(&events(), &t, 12, &provider("triage", "openai")).unwrap();
    assert_eq!((p.start_seq, p.agent_index), (2, 0));
    assert_eq!(p.reason.as_deref(), Some(REASON_EARLIER));
    // Clearing an absent provider is no change.
    let p = plan_fork(&events(), &t, 12, &provider("triage", " ")).unwrap();
    assert_eq!((p.start_seq, p.moved_from), (11, Some(12)));
}

#[test]
fn settling_drops_changes_to_the_agents_own_provider() {
    let t = team(AgentKind::Open);
    let both = |a: &str, b: &str| {
        let mut c = provider("triage", a);
        c.extend(provider("resolver", b));
        c
    };
    // Absent is the configured default: openrouter, or local:custom after the override.
    let mut c = both("openrouter", "local:custom");
    settle_changes(&t, &mut c, &ProviderConfig::default());
    assert_eq!(
        (&c["triage"].provider, c["resolver"].provider.as_deref()),
        (&None, Some("local:custom"))
    );
    let mut c = both("openrouter", "local:custom");
    let custom = ProviderConfig::with_upstream("http://127.0.0.1:9");
    settle_changes(&t, &mut c, &custom);
    assert_eq!(
        (c["triage"].provider.as_deref(), &c["resolver"].provider),
        (Some("openrouter"), &None)
    );
    // A blank provider is the default one too.
    let mut c = both(" ", "");
    settle_changes(&t, &mut c, &custom);
    assert_eq!(
        (&c["triage"].provider, &c["resolver"].provider),
        (&None, &None)
    );
}

#[test]
fn apply_changes_provider_rules() {
    let mut t = team(AgentKind::Open);
    t.agents[0].model = Some("old/model".into());
    t.agents[0].provider = Some("local:box".into());
    let ap = |c: Change| apply_changes(&t, &change("triage", c)).agents[0].clone();
    let to = |p: &str| Change {
        provider: Some(p.into()),
        ..Default::default()
    };
    // A new provider drops the old provider's model ...
    let a = ap(to("openai"));
    assert_eq!((a.provider.as_deref(), a.model), (Some("openai"), None));
    // ... unless the change names one.
    let a = ap(Change {
        model: Some("gpt-x".into()),
        ..to("openai")
    });
    assert_eq!(a.model.as_deref(), Some("gpt-x"));
    // The same provider keeps the model; blank is back to the default provider.
    assert_eq!(ap(to("local:box")).model.as_deref(), Some("old/model"));
    let a = ap(to(""));
    assert_eq!((a.provider, a.model), (None, None));
}

#[test]
fn kind_change_moves_to_agent_start() {
    let c = change(
        "resolver",
        Change {
            kind: Some(AgentKind::Codex),
            ..Default::default()
        },
    );
    let p = plan_fork(&events(), &team(AgentKind::Open), 11, &c).unwrap();
    assert_eq!(p.start_seq, 8);
    assert_eq!(p.moved_from, Some(11));
    assert_eq!(p.reason.as_deref(), Some("the agent's mode changes"));
    assert_eq!(p.agent_index, 1);
    assert_eq!(p.resume, None);
    assert_eq!(p.restore, base());
    assert_eq!(p.notes, vec!["N1".to_string()]);
}

#[test]
fn restore_is_last_snapshot_before_start() {
    let t = team(AgentKind::Open);
    let at = |seq| plan_fork(&events(), &t, seq, &no_changes()).unwrap();
    assert_eq!(at(13).restore, snap("snap_b"));
    assert_eq!(at(12).restore, snap("snap_a")); // moved to 11
    assert_eq!(at(11).restore, snap("snap_a"));
    assert_eq!(at(10).restore, base()); // moved to 9
    assert_eq!(at(9).restore, base());
    assert_eq!(at(3).restore, base());
}

#[test]
fn unsnapshotted_write_has_no_restore_point() {
    let mut evs = events();
    evs[11] = write(12, "resolver", None);
    let e = plan_fork(&evs, &team(AgentKind::Open), 13, &no_changes()).unwrap_err();
    assert!(matches!(e, ForkError::NoRestorePoint { seq: 12 }), "{e:?}");
    // Before the unsnapshotted write, the earlier snapshot is exact.
    let p = plan_fork(&evs, &team(AgentKind::Open), 11, &no_changes()).unwrap();
    assert_eq!(p.restore, snap("snap_a"));
    // A later snapshot is a full copy, so it is exact past an earlier unsnapshotted write.
    let mut evs = events();
    evs[9] = write(10, "resolver", None);
    let p = plan_fork(&evs, &team(AgentKind::Open), 13, &no_changes()).unwrap();
    assert_eq!(p.restore, snap("snap_b"));
}

/// The events of a branch forked at seq 11 (resume): prefix 1..=10, then the resumed resolver.
fn forked_events() -> Vec<Event> {
    let mut evs: Vec<Event> = events().into_iter().take(10).collect();
    evs.extend([
        start(11, "resolver"),
        mc(12, "resolver", "m12"),
        write(13, "resolver", Some("snap_c")),
        mc(14, "resolver", "m14"),
        ev(
            15,
            Some("resolver"),
            EventKind::AgentEnd,
            json!({"ok": true}),
        ),
        ev(16, None, EventKind::Check, json!({"passed": true})),
    ]);
    evs
}

#[test]
fn fork_of_a_fork_with_two_agent_starts() {
    let evs = forked_events();
    let t = team(AgentKind::Open);
    let p = plan_fork(&evs, &t, 13, &no_changes()).unwrap();
    assert_eq!(
        (p.start_seq, p.moved_from, p.agent_index),
        (12, Some(13), 1)
    );
    assert_eq!(p.resume, Some(msgs("m12")));
    assert_eq!(p.restore, snap("snap_a"));
    assert_eq!(p.notes, vec!["N1".to_string()]);
    let p = plan_fork(&evs, &t, 14, &no_changes()).unwrap();
    assert_eq!((p.start_seq, p.restore), (14, snap("snap_c")));
    // Restarting the agent goes to its real (first) start, before any of its writes.
    let c = change(
        "resolver",
        Change {
            kind: Some(AgentKind::ClaudeCode),
            ..Default::default()
        },
    );
    let p = plan_fork(&evs, &t, 13, &c).unwrap();
    assert_eq!((p.start_seq, p.resume, p.restore), (8, None, base()));
    // Forking at the resumed AgentStart restarts the agent from its first start.
    let p = plan_fork(&evs, &t, 11, &no_changes()).unwrap();
    assert_eq!((p.start_seq, p.moved_from, p.resume), (8, Some(11), None));
}

#[test]
fn other_steps_map_to_a_start_point() {
    let t = team(AgentKind::Open);
    // RunStart: the first agent's start.
    let p = plan_fork(&events(), &t, 1, &no_changes()).unwrap();
    assert_eq!(
        (p.start_seq, p.moved_from, p.agent_index, p.resume.clone()),
        (2, Some(1), 0, None)
    );
    assert!(p.reason.is_some());
    // An open agent's AgentStart stays, with no resume.
    let p = plan_fork(&events(), &t, 8, &no_changes()).unwrap();
    assert_eq!(
        (
            p.start_seq,
            p.moved_from,
            p.reason.clone(),
            p.resume.clone()
        ),
        (8, None, None, None)
    );
    assert_eq!(p.notes, vec!["N1".to_string()]);
    // A handoff is a tool call: redone from the model call that chose it.
    let p = plan_fork(&events(), &t, 6, &no_changes()).unwrap();
    assert_eq!((p.start_seq, p.agent_index), (5, 0));
    assert_eq!(
        p.reason.as_deref(),
        Some("a tool call is redone from the model call that chose it")
    );
    assert_end_steps_map_to_last_model_call(&t);
}

/// AgentEnd and Check: from the agent's last model call.
fn assert_end_steps_map_to_last_model_call(t: &Team) {
    let p = plan_fork(&events(), t, 14, &no_changes()).unwrap();
    assert_eq!((p.start_seq, p.moved_from), (13, Some(14)));
    let p = plan_fork(&events(), t, 15, &no_changes()).unwrap();
    assert_eq!(
        (p.start_seq, p.agent_index, p.restore),
        (13, 1, snap("snap_b"))
    );
}

/// Forking at `at` with the change `c` restarts the resolver from its first start.
fn assert_moves_back_to_resolver_start(t: &Team, c: &HashMap<String, Change>, at: i64) {
    let p = plan_fork(&events(), t, at, c).unwrap();
    assert_eq!(
        (p.start_seq, p.moved_from, p.agent_index),
        (8, Some(at), 1),
        "at {at}"
    );
    assert_eq!(p.reason.as_deref(), Some(REASON_EARLIER));
    assert_eq!((p.resume, p.restore), (None, base()));
    assert_eq!(p.notes, vec!["N1".to_string()]);
}

#[test]
fn change_to_finished_active_agent_moves_back() {
    let t = team(AgentKind::Open);
    let c = change(
        "resolver",
        Change {
            prompt: Some("New.".into()),
            ..Default::default()
        },
    );
    // At the Check (after the resolver's AgentEnd) and at its AgentEnd itself.
    for at in [15, 14] {
        assert_moves_back_to_resolver_start(&t, &c, at);
    }
    // Changes to both: the earliest finished one wins.
    let mut both = c.clone();
    both.insert(
        "triage".into(),
        Change {
            model: Some("m".into()),
            ..Default::default()
        },
    );
    let p = plan_fork(&events(), &t, 15, &both).unwrap();
    assert_eq!((p.start_seq, p.agent_index), (2, 0));
    // Before its AgentEnd the resolver is still active: a prompt change resumes.
    let p = plan_fork(&events(), &t, 13, &c).unwrap();
    assert_eq!((p.start_seq, p.moved_from), (13, None));
}

#[test]
fn apply_changes_model_rules() {
    let mut t = team(AgentKind::Open);
    t.agents[0].model = Some("old/model".into());
    let ap = |c: Change| apply_changes(&t, &change(&t.agents[0].id.clone(), c)).agents[0].clone();
    let to = |kind: AgentKind, model: Option<&str>| Change {
        kind: Some(kind),
        model: model.map(Into::into),
        ..Default::default()
    };
    // A mode change resets the model.
    let a = ap(to(AgentKind::Codex, None));
    assert_eq!((a.kind, a.model), (AgentKind::Codex, None));
    // An explicit model with the mode change is kept.
    let a = ap(to(AgentKind::Codex, Some("gpt-x")));
    assert_eq!(a.model.as_deref(), Some("gpt-x"));
    // Same kind keeps the model.
    let a = ap(to(AgentKind::Open, None));
    assert_eq!(a.model.as_deref(), Some("old/model"));
    // Empty or blank text clears.
    let model = |m: &str| {
        ap(Change {
            model: Some(m.into()),
            ..Default::default()
        })
        .model
    };
    assert_eq!(model(""), None);
    assert_eq!(model("  "), None);
    assert_eq!(model("new").as_deref(), Some("new"));
}

#[test]
fn same_or_empty_model_is_not_a_change() {
    let mut t = team(AgentKind::Open);
    t.agents[0].model = Some("m".into());
    let id = t.agents[0].id.clone();
    let same = Change {
        model: Some("m".into()),
        ..Default::default()
    };
    assert!(!is_change(&same, &t.agents[0]));
    let empty = Change {
        model: Some("".into()),
        ..Default::default()
    };
    assert!(is_change(&empty, &t.agents[0]));
    t.agents[0].model = None;
    assert!(!is_change(&empty, &t.agents[0]));
    assert!(is_change(&same, &t.agents[0]));
    // And plan_fork ignores it.
    let c = change(&id, empty);
    let p = plan_fork(&events(), &t, 11, &c).unwrap();
    assert_eq!((p.start_seq, p.moved_from), (11, None));
}

#[test]
fn a_read_only_change_is_carried_and_only_counts_when_it_differs() {
    let t = team(AgentKind::Open);
    let ro = |r: bool| Change {
        read_only: Some(r),
        ..Default::default()
    };
    assert!(!is_change(&ro(false), &t.agents[0]));
    assert!(is_change(&ro(true), &t.agents[0]));
    let id = t.agents[0].id.clone();
    let forked = apply_changes(&t, &change(&id, ro(true)));
    assert!(forked.agents[0].read_only && !forked.agents[1].read_only);
}

#[test]
fn same_kind_is_not_a_mode_change() {
    let t = team(AgentKind::Open);
    let c = change(
        "resolver",
        Change {
            kind: Some(AgentKind::Open),
            ..Default::default()
        },
    );
    let p = plan_fork(&events(), &t, 11, &c).unwrap();
    assert_eq!(
        (p.start_seq, p.moved_from, p.resume),
        (11, None, Some(msgs("m11")))
    );
    // Nor a change to a finished agent.
    let c = change(
        "triage",
        Change {
            kind: Some(AgentKind::Open),
            ..Default::default()
        },
    );
    let p = plan_fork(&events(), &t, 11, &c).unwrap();
    assert_eq!((p.start_seq, p.agent_index), (11, 1));
}

#[test]
fn bad_inputs_are_errors() {
    let t = team(AgentKind::Open);
    assert!(matches!(
        plan_fork(&events(), &t, 99, &no_changes()),
        Err(ForkError::UnknownSeq)
    ));
    let c = change(
        "nobody",
        Change {
            prompt: Some("x".into()),
            ..Default::default()
        },
    );
    assert!(
        matches!(plan_fork(&events(), &t, 11, &c), Err(ForkError::UnknownAgent(a)) if a == "nobody")
    );
    let evs: Vec<Event> = events()
        .into_iter()
        .filter(|e| e.kind != EventKind::Handoff)
        .collect();
    assert!(
        matches!(plan_fork(&evs, &t, 11, &no_changes()), Err(ForkError::MissingHandoff(a)) if a == "triage")
    );
}

#[test]
fn errors_read_well() {
    assert_eq!(
        ForkError::BranchRunning.to_string(),
        "the branch is still running"
    );
    assert!(ForkError::UnknownAgent("bob".into())
        .to_string()
        .contains("bob"));
    assert!(ForkError::NoRestorePoint { seq: 12 }
        .to_string()
        .contains("12"));
    assert!(ForkError::MissingHandoff("triage".into())
        .to_string()
        .contains("triage"));
}
