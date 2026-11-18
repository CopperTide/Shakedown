//! An agent's reasoning: what it thought, or said it would do, recorded as `reasoning` events.
//! A driver reports it (`POST …/agents/{agent}/reasoning`), and arenad pulls it out of the model
//! responses it records itself.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_items_have_no_payload() {
        assert_eq!(payload(&ReasoningItem::new(" \n", ReasoningKind::Step), "agent"), None);
    }

    #[test]
    fn long_text_is_cut_on_a_char_boundary() {
        let item = ReasoningItem::new("é".repeat(MAX_TEXT + 5), ReasoningKind::Thinking);
        let p = payload(&item, "agent").unwrap();
        assert_eq!(p["text"].as_str().unwrap().chars().count(), MAX_TEXT);
        assert_eq!(p["truncated"], json!(true));
    }
}
