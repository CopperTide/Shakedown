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

/// Reads `json` as `P` and writes it back byte for byte: `P`'s recorded form is pinned.
fn pinned<P: Serialize + DeserializeOwned>(json: &str) {
    let p: P = serde_json::from_str(json).unwrap_or_else(|e| panic!("{json}: {e}"));
    assert_eq!(serde_json::to_string(&p).unwrap(), json);
}

/// The payloads of steps and agents as recorded runs hold them: every key in its place, the
/// optional ones left out when empty, and the oldest shapes still read.
#[test]
fn recorded_step_payloads_keep_their_json() {
    use crate::model::{AgentEndPayload, ModelCallPayload, ToolCallPayload, WorldEventPayload};
    pinned::<ToolCallPayload>(
        r#"{"name":"get","args":{"id":1},"result":{"ok":true},"error":"no","reads":["a/1"],"writes":["a/2"],"internal":["_d/1"],"snapshot":"s1","snapshot_error":"bad","tool_call_id":"c1","part":"retail","saves":{"retail":"s1"},"clock_ms":5,"paused_ms":2,"duration_ms":1.5}"#,
    );
    pinned::<ToolCallPayload>(
        r#"{"name":"get","args":{},"result":null,"error":null,"reads":[],"writes":[],"snapshot":null,"tool_call_id":null}"#,
    );
    pinned::<WorldEventPayload>(
        r#"{"part":"clock","kind":"tick","text":"t","data":{"n":1},"clock_ms":7}"#,
    );
    pinned::<ModelCallPayload>(
        r#"{"request":{},"response":{},"usage":{},"cost_usd":0.25,"cost_source":"x","provider":"openai","provider_response":{"r":1}}"#,
    );
    pinned::<ModelCallPayload>(
        r#"{"request":{},"response":{},"usage":{},"cost_usd":null,"cost_source":null}"#,
    );
    pinned::<AgentEndPayload>(
        r#"{"ok":true,"error":null,"tokens":3,"prompt_tokens":1,"completion_tokens":2,"cost_usd":0.5,"cost_source":"x","seconds":1.5}"#,
    );
    pinned::<AgentEndPayload>(
        r#"{"ok":false,"error":"e","tokens":null,"cost_usd":null,"cost_source":null,"seconds":null}"#,
    );
}

/// A score as recorded runs hold it; one recorded before `unchecked` still reads.
#[test]
fn a_recorded_check_keeps_its_json() {
    use crate::model::CheckPayload;
    pinned::<CheckPayload>(
        r#"{"passed":false,"gold_db":"g","differing_rows":["t/1"],"checks":[{"name":"c","passed":false}],"final_saves":{"retail":"s"},"unchecked":["p"]}"#,
    );
    pinned::<CheckPayload>(
        r#"{"passed":true,"gold_db":null,"differing_rows":null,"unchecked":[]}"#,
    );
    let old: CheckPayload =
        serde_json::from_str(r#"{"passed":true,"gold_db":null,"differing_rows":null}"#).unwrap();
    assert!(old.unchecked.is_empty());
}

/// The payloads of what agents say to each other, as recorded runs hold them.
#[test]
fn recorded_talk_payloads_keep_their_json() {
    use crate::model::{
        DelegationPayload, FinishPayload, HandoffPayload, MessagePayload, ReportPayload,
    };
    pinned::<HandoffPayload>(r#"{"note":"n","to":"b"}"#);
    pinned::<HandoffPayload>(r#"{"note":"n"}"#);
    pinned::<MessagePayload>(r#"{"to":"*","text":"hi"}"#);
    pinned::<DelegationPayload>(r#"{"to":"b","task":"do"}"#);
    pinned::<ReportPayload>(r#"{"to":"a","text":"done","turn_end":true}"#);
    pinned::<ReportPayload>(r#"{"to":"a","text":"done"}"#);
    pinned::<FinishPayload>(r#"{"summary":"s"}"#);
}

/// A world view is written as the `json!` it was built with before it was typed.
#[test]
fn a_world_view_keeps_its_json() {
    let view = json!({"cards": [1, 2]});
    let typed = WorldViewPayload {
        seq_of: 4,
        view: view.clone(),
    };
    let built = json!({"seq_of": 4, "view": view});
    assert_eq!(serde_json::to_string(&typed).unwrap(), built.to_string());
    pinned::<WorldViewPayload>(r#"{"seq_of":4,"view":{"cards":[1,2]}}"#);
}
