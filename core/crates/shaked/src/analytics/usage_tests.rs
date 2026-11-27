use chrono::Duration;
use serde_json::json;

use super::*;
use crate::{
    model::{AgentKind, BranchStatus},
    testkit,
};

fn at(e: Event, ms: i64, base: DateTime<Utc>) -> Event {
    Event {
        at: base + Duration::milliseconds(ms),
        ..e
    }
}

fn team_branch() -> Branch {
    let mut b = testkit::branch(BranchStatus::Passed);
    let mut a = AgentSpec::new("a", AgentKind::ClaudeCode);
    a.model = Some("claude-sonnet-5-5".into());
    b.team.agents = vec![a];
    b
}

/// The agent wakes, reasons, then its call comes back 1.5 s after it woke.
fn woke_then_called(usage: serde_json::Value) -> Vec<Event> {
    let base = Utc::now();
    let call = json!({"provider": "openrouter", "request": {"model": "m"}, "usage": usage, "cost_usd": 0.5});
    vec![
        at(
            testkit::event(0, Some("a"), EventKind::Wake, json!({})),
            0,
            base,
        ),
        at(
            testkit::event(1, Some("a"), EventKind::Reasoning, json!({})),
            100,
            base,
        ),
        at(
            testkit::event(2, Some("a"), EventKind::ModelCall, call),
            1500,
            base,
        ),
    ]
}

#[test]
fn a_call_takes_its_tokens_reported_cost_and_time_since_the_agents_last_step() {
    let usage = json!({"prompt_tokens": 1000, "completion_tokens": 200,
        "prompt_tokens_details": {"cached_tokens": 600},
        "completion_tokens_details": {"reasoning_tokens": 50}});
    let calls = calls_of(&team_branch(), &woke_then_called(usage));
    assert_eq!(calls.len(), 1);
    let c = &calls[0];
    let named = (c.seq, c.model.as_deref(), c.provider.as_deref());
    assert_eq!(named, (2, Some("m"), Some("openrouter")));
    let tokens = (c.input, c.output, c.cached, c.reasoning);
    assert_eq!(tokens, (Some(1000), Some(200), Some(600), Some(50)));
    assert_eq!((c.cost_usd, c.think_ms), (Some(0.5), Some(1500.0)));
}

#[test]
fn an_unreported_cost_comes_from_the_list_price_of_the_agents_model() {
    let usage =
        json!({"input_tokens": 1_000_000, "cache_read_input_tokens": 0, "output_tokens": 100_000});
    let events = [testkit::event(
        0,
        Some("a"),
        EventKind::ModelCall,
        json!({"usage": usage}),
    )];
    let c = &calls_of(&team_branch(), &events)[0];
    assert_eq!(c.model.as_deref(), Some("claude-sonnet-5-5"));
    assert_eq!(
        (c.input, c.output, c.think_ms),
        (Some(1_000_000), Some(100_000), None)
    );
    assert!((c.cost_usd.unwrap() - 3.0).abs() < 1e-9, "{:?}", c.cost_usd);
}

#[test]
fn a_call_without_usage_has_no_tokens_or_cost() {
    let events = [testkit::event(
        0,
        Some("x"),
        EventKind::ModelCall,
        json!({"usage": null}),
    )];
    let c = &calls_of(&team_branch(), &events)[0];
    assert_eq!(
        (c.input, c.output, c.cost_usd, c.model.as_deref()),
        (None, None, None, None)
    );
}
