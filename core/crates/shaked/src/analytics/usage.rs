//! One lean row per model call of an ended branch (`call_usage`, migration 9), worked out with the
//! branch's figures so Explore and SQL never read a call's body: its tokens, its cost (as the
//! provider reported it, else at list price, as the run page's totals are) and its think time.
//!
//! Think time is an estimate, as on the run page's agent inspector: a call is recorded when its
//! answer comes back, so the time since the agent's previous step is taken as the call's.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{
    model::{AgentSpec, Branch, Event, EventKind},
    prices,
};

/// What one model call used.
#[derive(Debug, Clone, PartialEq)]
pub struct CallUsage {
    pub seq: i64,
    pub at: DateTime<Utc>,
    pub agent: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    /// The whole prompt, cached tokens included.
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub cached: Option<i64>,
    pub reasoning: Option<i64>,
    pub cost_usd: Option<f64>,
    /// Milliseconds since the agent's previous step; None for its first.
    pub think_ms: Option<f64>,
}

/// Steps that don't move an agent's clock: reasoning is written with its model call, the rest by
/// shaked.
fn passive(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Reasoning | EventKind::SavePoint | EventKind::WorldView | EventKind::Spans
    )
}

/// Every model call of `branch`'s `events` (in seq order), its own and any it inherited.
pub fn calls_of(branch: &Branch, events: &[Event]) -> Vec<CallUsage> {
    let mut last: HashMap<Option<&str>, DateTime<Utc>> = HashMap::new();
    let mut out = Vec::new();
    for e in events {
        let who = e.agent.as_deref();
        if e.kind == EventKind::ModelCall {
            let think = last
                .get(&who)
                .map(|t| (e.at - *t).num_milliseconds().max(0) as f64);
            out.push(call_of(e, spec_of(branch, who), think));
        }
        if !passive(e.kind) {
            last.insert(who, e.at);
        }
    }
    out
}

fn spec_of<'a>(branch: &'a Branch, agent: Option<&str>) -> Option<&'a AgentSpec> {
    branch
        .team
        .agents
        .iter()
        .find(|a| Some(a.id.as_str()) == agent)
}

fn text(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

fn call_of(e: &Event, spec: Option<&AgentSpec>, think_ms: Option<f64>) -> CallUsage {
    let p = &e.payload;
    let model = text(&p["request"]["model"])
        .or_else(|| text(&p["model"]))
        .or_else(|| spec.and_then(|s| s.model.clone()));
    let provider = text(&p["provider"])
        .or_else(|| spec.and_then(|s| prices::provider_of(s.kind, s.provider.as_deref())));
    let t = Tokens::of(&p["usage"]);
    let cost_usd = p["cost_usd"]
        .as_f64()
        .or_else(|| list_cost(provider.as_deref(), model.as_deref(), &t));
    CallUsage {
        seq: e.seq,
        at: e.at,
        agent: e.agent.clone(),
        provider,
        model,
        input: t.input,
        output: t.output,
        cached: t.cached,
        reasoning: t.reasoning,
        cost_usd,
        think_ms,
    }
}

/// A call without a reported cost at the flat list price the run page's totals use.
fn list_cost(provider: Option<&str>, model: Option<&str>, t: &Tokens) -> Option<f64> {
    let price = prices::table().find_flat(provider?, model?)?;
    Some(price.cost(t.input?, t.output.unwrap_or(0)))
}

/// A call's tokens, OpenAI-shaped (`prompt_tokens`, details) or Anthropic-shaped (`input_tokens`,
/// cache reads and writes apart).
#[derive(Debug, Default, PartialEq)]
struct Tokens {
    input: Option<i64>,
    output: Option<i64>,
    cached: Option<i64>,
    reasoning: Option<i64>,
}

impl Tokens {
    fn of(u: &Value) -> Tokens {
        let n = |v: &Value| v.as_i64();
        if let Some(prompt) = n(&u["prompt_tokens"]) {
            let cached = n(&u["prompt_tokens_details"]["cached_tokens"])
                .or_else(|| n(&u["cache_read_input_tokens"]));
            return Tokens {
                input: Some(prompt),
                output: n(&u["completion_tokens"]),
                cached,
                reasoning: n(&u["completion_tokens_details"]["reasoning_tokens"]),
            };
        }
        let read = n(&u["cache_read_input_tokens"]);
        let parts = [
            n(&u["input_tokens"]),
            read,
            n(&u["cache_creation_input_tokens"]),
        ];
        let input = parts
            .iter()
            .any(Option::is_some)
            .then(|| parts.iter().flatten().sum());
        Tokens {
            input,
            output: n(&u["output_tokens"]),
            cached: read,
            reasoning: None,
        }
    }
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
