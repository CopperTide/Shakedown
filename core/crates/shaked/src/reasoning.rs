//! An agent's reasoning: what it thought, or said it would do, recorded as `reasoning` events.
//! A driver reports it (`POST …/agents/{agent}/reasoning`), and shaked pulls it out of the model
//! responses it records itself.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{exec::BranchCtx, model::ReasoningPayload};

/// Most items one report may carry.
pub const MAX_ITEMS: usize = 100;
/// Longest text kept for one item, in characters; longer text is cut and marked `truncated`.
pub const MAX_TEXT: usize = 32_768;

/// Reported by the agent's driver.
pub const SOURCE_AGENT: &str = "agent";
/// Pulled from a model response that passed through shaked's gateway.
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
pub fn payload(item: &ReasoningItem, source: &str) -> Option<ReasoningPayload> {
    if item.text.trim().is_empty() {
        return None;
    }
    let cut = item.text.char_indices().nth(MAX_TEXT).map(|(i, _)| i);
    let text = cut.map_or(item.text.as_str(), |i| &item.text[..i]);
    Some(ReasoningPayload {
        text: text.to_string(),
        kind: item.kind,
        source: source.to_string(),
        model_call_seq: item.model_call_seq,
        truncated: cut.is_some(),
    })
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

/// A chat-completions message's thinking: `reasoning_content`, else `reasoning`, else each of
/// `reasoning_details`.
fn choice_thinking(m: &Value) -> Vec<ReasoningItem> {
    // The same thought often comes in more than one of these: the first that has it is kept.
    let first = ["reasoning_content", "reasoning"]
        .iter()
        .find_map(|k| m[*k].as_str().and_then(thinking));
    if let Some(item) = first {
        return vec![item];
    }
    let details = m["reasoning_details"].as_array().into_iter().flatten();
    details
        .filter_map(|d| {
            d["text"]
                .as_str()
                .or(d["summary"].as_str())
                .and_then(thinking)
        })
        .collect()
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
        if let Err(e) = ctx.record(Some(agent), &p).await {
            eprintln!("branch {}: recording reasoning: {e:#}", ctx.branch_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let texts = |m: Value| -> Vec<String> {
            let r = json!({"choices": [{"message": m}]});
            from_chat_response(&r, None)
                .into_iter()
                .map(|i| i.text)
                .collect()
        };
        // OpenRouter sends the same thought as `reasoning` and in `reasoning_details`: once.
        let both = json!({"reasoning": "a", "reasoning_details": [{"type": "reasoning.text", "text": "a"}]});
        assert_eq!(texts(both), ["a"]);
        // vLLM may send `reasoning_content` and `reasoning`: the first is kept.
        assert_eq!(
            texts(json!({"reasoning_content": "r", "reasoning": "r"})),
            ["r"]
        );
        // Details alone: each one.
        let details = json!({"reasoning_details": [{"text": "b"}, {"summary": "c"}]});
        assert_eq!(texts(details), ["b", "c"]);
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

    fn json_of(item: &ReasoningItem, source: &str) -> Option<Value> {
        payload(item, source).map(|p| serde_json::to_value(p).unwrap())
    }

    /// The payload's exact JSON (golden): `model_call_seq` and `truncated` only when they apply.
    #[test]
    fn a_payload_has_these_keys() {
        let item = ReasoningItem::new("Look first.", ReasoningKind::Plan);
        let plain = json!({"text": "Look first.", "kind": "plan", "source": "gateway"});
        assert_eq!(json_of(&item, SOURCE_GATEWAY), Some(plain));
        let long = ReasoningItem {
            model_call_seq: Some(4),
            ..ReasoningItem::new("x".repeat(MAX_TEXT + 1), ReasoningKind::Step)
        };
        let full = json!({"text": "x".repeat(MAX_TEXT), "kind": "step", "source": "agent",
                          "model_call_seq": 4, "truncated": true});
        assert_eq!(json_of(&long, SOURCE_AGENT), Some(full));
    }

    #[test]
    fn long_text_is_cut_on_a_char_boundary() {
        let item = ReasoningItem::new("é".repeat(MAX_TEXT + 5), ReasoningKind::Thinking);
        let p = json_of(&item, "agent").unwrap();
        assert_eq!(p["text"].as_str().unwrap().chars().count(), MAX_TEXT);
        assert_eq!(p["truncated"], json!(true));
    }
}
