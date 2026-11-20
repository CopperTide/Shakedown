use serde_json::json;

use super::*;
use crate::testkit;

fn decoded<P: EventPayload>(payload: Value) -> P {
    testkit::event(1, None, P::KIND, payload)
        .decode()
        .expect("of its kind")
}

#[test]
fn a_payload_reads_only_from_an_event_of_its_kind() {
    let e = testkit::event(1, None, EventKind::Pause, json!({}));
    assert_eq!(e.decode::<PausePayload>(), Some(PausePayload {}));
    assert_eq!(e.decode::<ResumePayload>(), None);
}

/// The optional keys are written only when they say something; `Pause` is `{}`.
#[test]
fn optional_keys_are_left_out() {
    let start = AgentStartPayload {
        name: "a".into(),
        kind: Some(AgentKind::Open),
        ..AgentStartPayload::default()
    };
    let want = json!({"name": "a", "kind": "open", "model": null});
    assert_eq!(serde_json::to_value(&start).unwrap(), want);
    assert_eq!(serde_json::to_value(PausePayload {}).unwrap(), json!({}));
    let idle = IdlePayload {
        ok: Some(true),
        text: "done".into(),
        error: None,
    };
    let want = json!({"ok": true, "text": "done"});
    assert_eq!(serde_json::to_value(&idle).unwrap(), want);
    let rs = serde_json::to_value(RunStartPayload::default()).unwrap();
    for key in ["scenario", "max_cost_usd", "connection"] {
        assert!(rs.get(key).is_none(), "{key}: {rs}");
    }
}

/// A run recorded before integrations and runners: only `task_id` and `team`.
#[test]
fn an_old_run_start_reads_with_defaults() {
    let team = json!({"id": "t", "name": "T", "agents": []});
    let rs: RunStartPayload = decoded(json!({"task_id": "27", "team": team}));
    assert_eq!(rs.task_id.as_deref(), Some("27"));
    assert_eq!(rs.team.map(|t| t.id), Some("t".to_string()));
    let none = (&rs.benchmark, &rs.world, &rs.saves, &rs.runner, &rs.handoff);
    assert_eq!(none, (&None, &None, &None, &None, &None));
    assert_eq!((rs.seed, rs.fingerprints.len(), rs.tools.len()), (0, 0, 0));
}

/// A key of another shape reads as its default, and the other keys still read.
#[test]
fn a_key_of_another_shape_reads_as_its_default() {
    let rs: RunStartPayload = decoded(json!({
        "task_id": 27, "seed": -1, "saves": ["x"], "runner": "nobody", "tools": [],
        "benchmark": "retail", "max_cost_usd": "1", "handoff": "required"
    }));
    assert_eq!((rs.task_id, rs.seed, rs.saves), (None, 0, None));
    assert_eq!(
        (rs.runner, rs.max_cost_usd, rs.tools.len()),
        (None, None, 0)
    );
    assert_eq!(rs.benchmark.as_deref(), Some("retail"));
    assert_eq!(rs.handoff.as_deref(), Some("required"));
    let start: AgentStartPayload = decoded(json!({"name": "a", "kind": "bedrock"}));
    assert_eq!((start.name.as_str(), start.kind), ("a", None));
    let idle: IdlePayload = decoded(json!({"text": "t"}));
    assert_eq!((idle.ok, idle.text.as_str()), (None, "t"));
    let wake: WakePayload = decoded(json!({"reason": "idle", "mail": "x"}));
    assert_eq!(
        (wake.reason.as_str(), wake.mail, wake.idle_wake),
        ("idle", vec![], None)
    );
}

/// A payload that isn't an object reads as the defaults.
#[test]
fn a_payload_that_is_not_an_object_reads_as_defaults() {
    let rs: RunStartPayload = decoded(json!(null));
    assert_eq!(rs, RunStartPayload::default());
    let r: ReasoningPayload = decoded(json!("text"));
    assert_eq!(r, ReasoningPayload::default());
}

/// The `overhead_ms` the store adds to every payload is not part of any.
#[test]
fn the_overhead_the_store_adds_is_ignored() {
    let p = json!({"paused_ms": 40, "overhead_ms": {"record": 1.0}});
    assert_eq!(decoded::<ResumePayload>(p).paused_ms, 40);
}
