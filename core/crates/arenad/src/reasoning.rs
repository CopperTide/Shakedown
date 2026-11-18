//! An agent's reasoning: what it thought, or said it would do, recorded as `reasoning` events.
//! A driver reports it (`POST …/agents/{agent}/reasoning`), and arenad pulls it out of the model
//! responses it records itself.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{exec::BranchCtx, model::EventKind, store::Store};

/// Most items one report may carry.
pub const MAX_ITEMS: usize = 100;
/// Longest text kept for one item, in characters; longer text is cut and marked `truncated`.
pub const MAX_TEXT: usize = 32_768;

/// Reported by the agent's driver.
pub const SOURCE_AGENT: &str = "agent";
/// Pulled from a model response that passed through arenad's gateway.
pub const SOURCE_GATEWAY: &str = "gateway";
/// Pulled from Claude Code's stream.
pub const SOURCE_CLAUDE_CODE: &str = "claude_code";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningKind {
    /// The model's own thinking.
    #[default]
    Thinking,
    /// What the agent says between tool calls.
    Step,
    /// A plan it laid out.
    Plan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningItem {
    pub text: String,
    #[serde(default)]
    pub kind: ReasoningKind,
    /// The `model_call` event this came from, when known.
    #[serde(default)]
    pub model_call_seq: Option<i64>,
}

impl ReasoningItem {
    pub fn new(text: impl Into<String>, kind: ReasoningKind) -> ReasoningItem {
        ReasoningItem {
            text: text.into(),
            kind,
            model_call_seq: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReasoningReport {
    pub items: Vec<ReasoningItem>,
}

/// The `reasoning` event's payload for `item`, or `None` when its text is blank.
pub fn payload(item: &ReasoningItem, source: &str) -> Option<Value> {
    if item.text.trim().is_empty() {
        return None;
    }
    let cut = item.text.char_indices().nth(MAX_TEXT).map(|(i, _)| i);
    let text = cut.map_or(item.text.as_str(), |i| &item.text[..i]);
    let mut v = json!({"text": text, "kind": item.kind, "source": source});
    if let Some(seq) = item.model_call_seq {
        v["model_call_seq"] = json!(seq);
    }
    if cut.is_some() {
        v["truncated"] = json!(true);
    }
    Some(v)
}

fn thinking(text: &str) -> Option<ReasoningItem> {
    (!text.trim().is_empty()).then(|| ReasoningItem::new(text, ReasoningKind::Thinking))
}

/// The reasoning in a chat-completions response: each choice's `reasoning_content`, `reasoning`
/// and `reasoning_details[]` (`text` or `summary`). When it has none, the `thinking` blocks of
/// the provider's own (Anthropic-shape) response. Redacted thinking is never kept. Then what the
/// model says alongside its tool calls, as steps.
pub fn from_chat_response(canonical: &Value, provider_raw: Option<&Value>) -> Vec<ReasoningItem> {
    let choices = || canonical["choices"].as_array().into_iter().flatten();
    let mut out: Vec<ReasoningItem> = choices()
        .flat_map(|c| choice_thinking(&c["message"]))
        .collect();
    if out.is_empty() {
        let blocks = provider_raw.and_then(|r| r["content"].as_array());
        let thought = blocks
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "thinking");
        out.extend(thought.filter_map(|b| b["thinking"].as_str().and_then(thinking)));
    }
    out.extend(choices().filter_map(|c| words_with_calls(&c["message"])));
    out
}

/// A chat-completions message's thinking: `reasoning_content`, `reasoning`, `reasoning_details`.
fn choice_thinking(m: &Value) -> Vec<ReasoningItem> {
    let mut out = vec![];
    for key in ["reasoning_content", "reasoning"] {
        out.extend(m[key].as_str().and_then(thinking));
    }
    for d in m["reasoning_details"].as_array().into_iter().flatten() {
        out.extend(
            d["text"]
                .as_str()
                .or(d["summary"].as_str())
                .and_then(thinking),
        );
    }
    out
}

/// What a message says alongside its tool calls, as a step.
fn words_with_calls(m: &Value) -> Option<ReasoningItem> {
    let calls = m["tool_calls"].as_array().is_some_and(|c| !c.is_empty());
    let said = m["content"]
        .as_str()
        .filter(|t| calls && !t.trim().is_empty());
    said.map(|t| ReasoningItem::new(t, ReasoningKind::Step))
}

/// The reasoning in one of Claude Code's assistant messages: its `thinking` blocks, and its `text`
/// blocks as steps.
pub fn from_claude_message(message: &Value) -> Vec<ReasoningItem> {
    let blocks = message["content"].as_array().into_iter().flatten();
    blocks
        .filter_map(|b| match b["type"].as_str() {
            Some("thinking") => b["thinking"].as_str().and_then(thinking),
            Some("text") => b["text"]
                .as_str()
                .filter(|t| !t.trim().is_empty())
                .map(|t| ReasoningItem::new(t, ReasoningKind::Step)),
            _ => None,
        })
        .collect()
}

/// Records `items` as the agent's reasoning from `source`, each tied to the model call `seq` when
/// there is one. A failure is logged, never raised: the reasoning is a note on a call that
/// already happened.
pub async fn record(
    ctx: &BranchCtx,
    store: &Store,
    agent: &str,
    items: Vec<ReasoningItem>,
    source: &str,
    seq: Option<i64>,
) {
    for mut item in items {
        item.model_call_seq = item.model_call_seq.or(seq);
        let Some(p) = payload(&item, source) else {
            continue;
        };
        if let Err(e) = ctx
            .append(store, Some(agent), EventKind::Reasoning, p)
            .await
        {
            eprintln!("branch {}: recording reasoning: {e:#}", ctx.branch_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(items: &[ReasoningItem]) -> Vec<(&str, ReasoningKind)> {
        items.iter().map(|i| (i.text.as_str(), i.kind)).collect()
    }

    #[test]
    fn deepseek_style_reasoning_content_is_thinking() {
        let r = json!({"choices": [{"message": {"content": "hi", "reasoning_content": "r"}}]});
        assert_eq!(
            texts(&from_chat_response(&r, None)),
            [("r", ReasoningKind::Thinking)]
        );
    }

    #[test]
    fn openrouter_reasoning_string_and_details() {
        let r = json!({"choices": [{"message": {
            "reasoning": "a",
            "reasoning_details": [{"type": "reasoning.text", "text": "b"}, {"summary": "c"}],
        }}]});
        let got: Vec<_> = from_chat_response(&r, None)
            .into_iter()
            .map(|i| i.text)
            .collect();
        assert_eq!(got, ["a", "b", "c"]);
    }

    #[test]
    fn anthropic_thinking_blocks_from_the_raw_response() {
        let canonical = json!({"choices": [{"message": {"content": "hi"}}]});
        let raw = json!({"content": [
            {"type": "thinking", "thinking": "t"},
            {"type": "redacted_thinking", "data": "x"},
            {"type": "text", "text": "hi"},
        ]});
        let got = from_chat_response(&canonical, Some(&raw));
        assert_eq!(texts(&got), [("t", ReasoningKind::Thinking)]);
    }

    #[test]
    fn words_alongside_tool_calls_are_a_step() {
        let r = json!({"choices": [{"message": {
            "content": "Checking stock first.",
            "tool_calls": [{"id": "c1", "function": {"name": "get_stock", "arguments": "{}"}}],
        }}]});
        assert_eq!(
            texts(&from_chat_response(&r, None)),
            [("Checking stock first.", ReasoningKind::Step)]
        );
    }

    #[test]
    fn no_reasoning_gives_nothing() {
        let r = json!({"choices": [{"message": {"content": "hi"}}]});
        assert!(from_chat_response(&r, None).is_empty());
    }

    #[test]
    fn claude_code_thinking_and_text_blocks() {
        let m = json!({"content": [
            {"type": "thinking", "thinking": "plan"},
            {"type": "tool_use", "id": "t1", "name": "x", "input": {}},
            {"type": "text", "text": "Reserving now."},
        ]});
        assert_eq!(
            texts(&from_claude_message(&m)),
            [
                ("plan", ReasoningKind::Thinking),
                ("Reserving now.", ReasoningKind::Step)
            ]
        );
    }

    #[test]
    fn blank_items_have_no_payload() {
        assert_eq!(
            payload(&ReasoningItem::new(" \n", ReasoningKind::Step), "agent"),
            None
        );
    }

    #[test]
    fn long_text_is_cut_on_a_char_boundary() {
        let item = ReasoningItem::new("é".repeat(MAX_TEXT + 5), ReasoningKind::Thinking);
        let p = payload(&item, "agent").unwrap();
        assert_eq!(p["text"].as_str().unwrap().chars().count(), MAX_TEXT);
        assert_eq!(p["truncated"], json!(true));
    }
}
