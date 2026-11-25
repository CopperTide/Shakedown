//! What the explainer reads about a step, and its answer checked against it.

use super::*;
use crate::model::{AgentKind, AgentSpec};
use serde_json::json;

fn event(seq: i64, agent: Option<&str>, kind: EventKind, payload: Value) -> Event {
    Event {
        branch_id: "b".into(),
        seq,
        at: chrono::Utc::now(),
        agent: agent.map(str::to_string),
        kind,
        payload,
    }
}

const POLICY: &str = "Refunds over $500 need a manager's approval.";

/// A run: the agent reads an order, its model is asked with the policy and the tool result, and
/// it thinks before refunding.
fn events() -> Vec<Event> {
    let messages = json!([
        {"role": "system", "content": format!("You are ops. {POLICY}")},
        {"role": "user", "content": "Refund order 7."},
        {"role": "tool", "content": "{\"order\": 7, \"total\": 640}"},
    ]);
    let response = json!({"choices": [{"message": {"role": "assistant", "content": "I will ask a manager first."}}]});
    vec![
        event(1, None, EventKind::RunStart, json!({"seed": 1})),
        event(
            2,
            Some("ops"),
            EventKind::ToolCall,
            json!({"tool": "orders.get", "result": {"total": 640}}),
        ),
        event(
            3,
            Some("ops"),
            EventKind::ModelCall,
            json!({"request": {"messages": messages}, "response": response}),
        ),
        event(
            4,
            Some("ops"),
            EventKind::Reasoning,
            json!({"text": "The total is over the limit,\n so I need approval.", "model_call_seq": 3}),
        ),
    ]
}

fn team() -> Team {
    let mut a = AgentSpec::new("ops", AgentKind::Open);
    a.prompt = "Handle refunds.".into();
    Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![a],
    }
}

fn ctx() -> AskContext {
    context(
        &events(),
        4,
        &team(),
        Some("Ticket 7: refund"),
        "Why approval?",
    )
    .expect("step 4")
}

fn ids(c: &AskContext) -> Vec<&str> {
    c.sources.iter().map(|s| s.id.as_str()).collect()
}

#[test]
fn a_thought_reads_the_messages_of_the_call_it_came_from_and_the_steps_before() {
    let c = ctx();
    assert_eq!(c.step.kind, "reasoning");
    assert_eq!(c.step.agent.as_deref(), Some("ops"));
    assert_eq!(
        ids(&c),
        [
            "s4",
            "ticket",
            "prompt",
            "m3.0",
            "m3.1",
            "m3.2",
            "m3.response",
            "s2"
        ]
    );
    let system = &c.sources[3];
    assert_eq!(
        (system.step, system.field.as_str()),
        (Some(3), "request.messages[0] (system)")
    );
    assert!(system.text.contains(POLICY));
    assert_eq!(c.sources_left_out, 0);
    assert!(context(&events(), 99, &team(), None, "q").is_none());
}

#[test]
fn a_tool_call_reads_the_agent_s_latest_call_before_it() {
    let mut ev = events();
    ev.push(event(
        5,
        Some("ops"),
        EventKind::ToolCall,
        json!({"tool": "approvals.ask"}),
    ));
    let c = context(&ev, 5, &team(), None, "q").unwrap();
    assert!(
        ids(&c).contains(&"m3.0") && ids(&c).contains(&"s4"),
        "{:?}",
        ids(&c)
    );
}

#[test]
fn sources_beyond_the_budget_are_left_out_and_counted() {
    let big = "x".repeat(3_900);
    let messages: Vec<Value> = (0..30)
        .map(|i| json!({"role": "user", "content": format!("{i} {big}")}))
        .collect();
    let mut ev = events();
    ev[2].payload["request"]["messages"] = json!(messages);
    let c = context(&ev, 4, &team(), None, "q").unwrap();
    assert!(c.sources_left_out > 0);
    assert_eq!(c.sources[0].id, "s4", "the step itself is always kept");
    assert!(
        ids(&c).contains(&"m3.0") && ids(&c).contains(&"m3.29"),
        "the first and last messages first"
    );
    let shown: usize = c.sources.iter().map(|s| s.text.len()).sum();
    assert!(shown <= MAX_SOURCES_BYTES, "{shown}");
}

#[test]
fn evidence_is_kept_only_when_its_source_has_the_quote_word_for_word() {
    let c = ctx();
    let answer = json!({
        "answer": "The system prompt sets a $500 limit and the order is $640.",
        "evidence": [
            {"source": "m3.0", "quote": format!("\"{POLICY}\""), "why": "the rule"},
            {"source": "s4", "quote": "over the limit, so I need", "why": "line breaks squash"},
            {"source": "m3.2", "quote": "the total is 900", "why": "made up"},
            {"source": "m9.0", "quote": "Refund order 7.", "why": "no such source"},
        ],
        "speculation": [{"text": "Likely anchored on the policy line.", "based_on": ["m3.0", "nope"]}],
    });
    let (e, issues) = check(&answer, &c).unwrap();
    let kept: Vec<(&str, Option<i64>)> = e
        .evidence
        .iter()
        .map(|x| (x.source.as_str(), x.step))
        .collect();
    assert_eq!(kept, [("m3.0", Some(3)), ("s4", Some(4))]);
    assert_eq!(e.evidence[0].quote, POLICY);
    assert_eq!(e.evidence[0].field, "request.messages[0] (system)");
    assert_eq!(e.speculation[0].based_on, ["m3.0"]);
    assert_eq!(issues.len(), 3, "{issues:?}");
    assert!(
        issues[0].contains("m3.2") && issues[1].contains("m9.0"),
        "{issues:?}"
    );
    assert!(
        check(&json!({"evidence": []}), &c).is_err(),
        "an answer is required"
    );
}

#[test]
fn the_prompt_is_the_data_file_and_the_schema_keeps_evidence_apart() {
    let p = super::super::prompt::ask_prompt(&ctx());
    assert!(p.system.starts_with("You explain one step"));
    assert!(p.user.contains("Why approval?") && p.user.contains("m3.response"));
    assert!(!p.user.contains("\"full\""), "the unshown text stays out");
    let required = &p.schema["required"];
    assert_eq!(required, &json!(["answer", "evidence", "speculation"]));
}
