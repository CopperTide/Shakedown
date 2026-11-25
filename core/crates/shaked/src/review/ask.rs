//! Asking about one step: what the explainer reads, and its answer checked. It reads the
//! *sources*, what the agent had at that step (the messages sent to its model for it, that
//! model's response, the agent's prompt, the ticket, the earlier steps), each with its step and
//! field. Its answer keeps evidence apart from speculation: evidence is a quote of one source, kept
//! only when that source has it word for word; speculation is its labelled inference. Starting a
//! question and storing its answer are [`super::asking`].

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::output::{list, text};
use crate::model::{Event, EventKind, RunStartPayload, Team};
use crate::text::cut;

/// The longest question, in characters.
pub const MAX_QUESTION: usize = 2000;
/// What the sources' shown texts may come to, in bytes.
const MAX_SOURCES_BYTES: usize = 60_000;
/// The most of one source shown; the step itself gets twice this.
const SOURCE_BYTES: usize = 4_000;
/// The most of an earlier step shown.
const EARLIER_BYTES: usize = 1_200;
/// The earlier steps offered at most (the latest).
const MAX_EARLIER: usize = 40;
/// The messages of the step's model call offered first: the first one and the last few.
const LAST_MESSAGES: usize = 6;
/// A quote's bounds, in characters.
const QUOTE_CHARS: std::ops::RangeInclusive<usize> = 3..=600;
const MAX_EVIDENCE: usize = 8;
const MAX_GUESSES: usize = 5;

/// One thing the agent had at the step, as the explainer may cite it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Source {
    /// `s<seq>` for a step, `m<seq>.<i>` for message `i` sent in model call `seq`,
    /// `m<seq>.response` for its response, `ticket`, `prompt`, `scenario`.
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<i64>,
    /// Where in the record it is: `request.messages[2] (tool)`, `response`, `tool_call by ops`.
    pub field: String,
    /// What the explainer is shown (cut).
    pub text: String,
    /// All of it, what quotes are checked against.
    #[serde(skip)]
    full: String,
}

/// The step asked about.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepLine {
    pub seq: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub kind: &'static str,
}

/// What the explainer reads: the question, the step and the sources.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AskContext {
    pub question: String,
    pub step: StepLine,
    pub sources: Vec<Source>,
    /// Sources that did not fit.
    #[serde(skip_serializing_if = "is_zero")]
    pub sources_left_out: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// One quote of the record, with where it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    pub step: Option<i64>,
    pub field: String,
    pub quote: String,
    pub why: String,
}

/// A hypothesis the record cannot show, and the sources it rests on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Guess {
    pub text: String,
    pub based_on: Vec<String>,
}

/// The checked answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    pub answer: String,
    pub evidence: Vec<Evidence>,
    pub speculation: Vec<Guess>,
}

/// What the explainer reads about step `seq` of `events`, or `None` when there is no such step.
pub fn context(
    events: &[Event],
    seq: i64,
    team: &Team,
    ticket: Option<&str>,
    question: &str,
) -> Option<AskContext> {
    let step = events.iter().find(|e| e.seq == seq)?;
    let (sources, sources_left_out) = within(ranked(events, step, team, ticket), MAX_SOURCES_BYTES);
    let step = StepLine {
        seq,
        agent: step.agent.clone(),
        kind: step.kind.as_str(),
    };
    Some(AskContext {
        question: question.to_string(),
        step,
        sources,
        sources_left_out,
    })
}

/// Every source with its rank: the lower ranks are kept first when they don't all fit.
fn ranked(events: &[Event], step: &Event, team: &Team, ticket: Option<&str>) -> Vec<(u8, Source)> {
    let seq = step.seq;
    let full = text_of(step);
    let mut out = vec![(
        0,
        source(
            &format!("s{seq}"),
            Some(seq),
            field_of(step),
            full,
            2 * SOURCE_BYTES,
        ),
    )];
    out.extend(given(step, team, ticket).map(|s| (1, s)));
    if let Some(call) = model_call_for(events, step) {
        out.extend(call_sources(call, call.seq == seq));
    }
    out.extend(earlier(events, seq).map(|s| (3, s)));
    out.extend(scenario(events).map(|s| (5, s)));
    out
}

/// What the agent was given before the run: the ticket and its prompt.
fn given(step: &Event, team: &Team, ticket: Option<&str>) -> impl Iterator<Item = Source> {
    let ticket = ticket.map(|t| source("ticket", None, "ticket".into(), t.into(), SOURCE_BYTES));
    let spec = step
        .agent
        .as_deref()
        .and_then(|a| team.agents.iter().find(|s| s.id == a));
    let prompt = spec.filter(|s| !s.prompt.trim().is_empty()).map(|s| {
        let field = format!("the prompt of agent {}", s.id);
        source("prompt", None, field, s.prompt.clone(), SOURCE_BYTES)
    });
    ticket.into_iter().chain(prompt)
}

fn source(id: &str, step: Option<i64>, field: String, full: String, max: usize) -> Source {
    Source {
        id: id.to_string(),
        step,
        field,
        text: cut(&full, max),
        full,
    }
}

fn field_of(e: &Event) -> String {
    match &e.agent {
        Some(a) => format!("{} by {a}", e.kind.as_str()),
        None => e.kind.as_str().to_string(),
    }
}

/// A step as text: a thought's text, a model call's response, else its payload as JSON.
fn text_of(e: &Event) -> String {
    match e.kind {
        EventKind::Reasoning => str_or_json(&e.payload["text"]),
        EventKind::ModelCall => response_text(&e.payload["response"]),
        _ => e.payload.to_string(),
    }
}

fn str_or_json(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

/// The model call the step came from: the step itself, the call a thought names, else the
/// agent's latest call before it.
fn model_call_for<'a>(events: &'a [Event], step: &'a Event) -> Option<&'a Event> {
    if step.kind == EventKind::ModelCall {
        return Some(step);
    }
    let mut calls = events
        .iter()
        .rev()
        .filter(|e| e.kind == EventKind::ModelCall);
    match step.payload["model_call_seq"].as_i64() {
        Some(n) => calls.find(|e| e.seq == n),
        None => calls.find(|e| e.seq < step.seq && e.agent.is_some() && e.agent == step.agent),
    }
}

/// The messages sent in model call `call` (its first and last few ranked first) and, unless the
/// call is the step itself, its response.
fn call_sources(call: &Event, is_step: bool) -> Vec<(u8, Source)> {
    let messages = list(&call.payload["request"], "messages");
    let n = messages.len();
    let mut out: Vec<(u8, Source)> = messages
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let rank = if i == 0 || i + LAST_MESSAGES >= n {
                2
            } else {
                4
            };
            let role = m["role"].as_str().unwrap_or("message");
            let field = format!("request.messages[{i}] ({role})");
            let id = format!("m{}.{i}", call.seq);
            (
                rank,
                source(&id, Some(call.seq), field, message_text(m), SOURCE_BYTES),
            )
        })
        .collect();
    if !is_step {
        let id = format!("m{}.response", call.seq);
        let full = response_text(&call.payload["response"]);
        out.push((
            2,
            source(&id, Some(call.seq), "response".into(), full, SOURCE_BYTES),
        ));
    }
    out
}

/// A chat message as text: its content (the text parts of a list) and any tool calls.
fn message_text(m: &Value) -> String {
    let content = match &m["content"] {
        Value::String(s) => s.clone(),
        Value::Array(parts) => {
            let texts = parts.iter().filter_map(|p| p["text"].as_str());
            texts.collect::<Vec<_>>().join("\n")
        }
        _ => String::new(),
    };
    let calls = m.get("tool_calls").filter(|c| !c.is_null());
    match (content.is_empty(), calls) {
        (_, Some(c)) => format!("{content}\n{c}").trim().to_string(),
        (true, None) => m.to_string(),
        (false, None) => content,
    }
}

/// A chat completion's text: its first choice's message, else the whole response.
fn response_text(r: &Value) -> String {
    match r["choices"][0].get("message") {
        Some(m) => message_text(m),
        None => r.to_string(),
    }
}

/// The latest steps before `seq` (no model calls: the messages above carry the agent's own).
fn earlier(events: &[Event], seq: i64) -> impl Iterator<Item = Source> + '_ {
    let skip = |k: EventKind| {
        k.is_bookkeeping() || matches!(k, EventKind::ModelCall | EventKind::RunStart)
    };
    let before: Vec<&Event> = events
        .iter()
        .filter(|e| e.seq < seq && !skip(e.kind))
        .collect();
    let from = before.len().saturating_sub(MAX_EARLIER);
    before.into_iter().skip(from).map(|e| {
        source(
            &format!("s{}", e.seq),
            Some(e.seq),
            field_of(e),
            text_of(e),
            EARLIER_BYTES,
        )
    })
}

fn scenario(events: &[Event]) -> Option<Source> {
    let start = RunStartPayload::of(events)?;
    let s = start.scenario.filter(|s| !s.is_null())?;
    Some(source(
        "scenario",
        None,
        "the run's scenario".into(),
        s.to_string(),
        SOURCE_BYTES,
    ))
}

/// The sources that fit in `budget` bytes, lower ranks first, in their order; and how many were
/// left out.
fn within(ranked: Vec<(u8, Source)>, budget: usize) -> (Vec<Source>, usize) {
    let mut order: Vec<usize> = (0..ranked.len()).collect();
    order.sort_by_key(|&i| ranked[i].0);
    let (mut used, mut keep) = (0, vec![false; ranked.len()]);
    for i in order {
        let size = ranked[i].1.text.len() + ranked[i].1.field.len() + 32;
        if used + size <= budget {
            used += size;
            keep[i] = true;
        }
    }
    let left_out = keep.iter().filter(|k| !**k).count();
    let kept = ranked
        .into_iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|((_, s), _)| s);
    (kept.collect(), left_out)
}

/// Whitespace runs as one space, so a quote matches across line breaks and indentation.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A quote as given, without the quotation marks around it.
fn unquoted(q: &str) -> &str {
    q.trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '“' | '”' | '‘' | '’'))
        .trim()
}

/// The answer checked against `ctx`: evidence whose quote its source has word for word (the
/// step and field taken from the source), speculation citing only known sources; and what
/// checking dropped, in plain words.
pub fn check(v: &Value, ctx: &AskContext) -> Result<(Explanation, Vec<String>)> {
    let Some(answer) = text(v, "answer") else {
        bail!("the explainer's answer has no answer");
    };
    let mut issues = vec![];
    let evidence = (list(v, "evidence").iter().take(MAX_EVIDENCE))
        .filter_map(|e| evidence(e, ctx, &mut issues))
        .collect();
    let speculation = (list(v, "speculation").iter().take(MAX_GUESSES))
        .filter_map(|g| guess(g, ctx, &mut issues))
        .collect();
    let explained = Explanation {
        answer,
        evidence,
        speculation,
    };
    Ok((explained, issues))
}

fn find<'a>(ctx: &'a AskContext, id: &str) -> Option<&'a Source> {
    ctx.sources.iter().find(|s| s.id == id.trim())
}

/// One evidence item, when its source has its quote.
fn evidence(e: &Value, ctx: &AskContext, issues: &mut Vec<String>) -> Option<Evidence> {
    let id = e["source"].as_str().unwrap_or_default();
    let quote = unquoted(e["quote"].as_str().unwrap_or_default());
    let Some(s) = find(ctx, id) else {
        issues.push(format!("dropped evidence citing {id:?}: no such source"));
        return None;
    };
    let fits = QUOTE_CHARS.contains(&quote.chars().count());
    if !fits || !squash(&s.full).contains(&squash(quote)) {
        issues.push(format!(
            "dropped evidence from {}: the quote is not in it word for word",
            s.id
        ));
        return None;
    }
    Some(Evidence {
        source: s.id.clone(),
        step: s.step,
        field: s.field.clone(),
        quote: quote.to_string(),
        why: text(e, "why").unwrap_or_default(),
    })
}

/// One hypothesis, citing only sources there are.
fn guess(g: &Value, ctx: &AskContext, issues: &mut Vec<String>) -> Option<Guess> {
    let text = text(g, "text")?;
    let mut based_on = vec![];
    for b in list(g, "based_on").iter().filter_map(Value::as_str) {
        match find(ctx, b) {
            Some(s) => based_on.push(s.id.clone()),
            None => issues.push(format!("dropped {b:?} from a hypothesis: no such source")),
        }
    }
    Some(Guess { text, based_on })
}

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;
