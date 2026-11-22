//! The canonical (OpenAI chat-completions) format as the Claude and Bedrock adapters read and
//! write it: a request's messages, their text and tool calls, the alternating turns both native
//! APIs want, and the one-choice `chat.completion` answer. Pure.

use serde_json::{json, Value};

/// The request's messages, in order.
pub(super) fn messages(chat: &Value) -> impl Iterator<Item = &Value> {
    chat["messages"].as_array().into_iter().flatten()
}

/// True for a `system` or `developer` message: part of the system prompt.
pub(super) fn is_system(m: &Value) -> bool {
    matches!(m["role"].as_str(), Some("system" | "developer"))
}

/// A message's content as text: a string, or its parts' text one per line, each part read by
/// `part` (an adapter drops a part that isn't text, or says where it was).
pub(super) fn content_text(content: &Value, part: impl Fn(&Value) -> Option<String>) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().filter_map(part).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// Appends `blocks` as a `role` turn, into the last turn when it has the same role: turns
/// alternate, and the results of parallel tool calls go back in one user turn. No blocks, no
/// turn.
pub(super) fn push_turn(turns: &mut Vec<Value>, role: &str, blocks: Vec<Value>) {
    if blocks.is_empty() {
        return;
    }
    if let Some(last) = turns.last_mut().filter(|t| t["role"] == role) {
        if let Some(content) = last["content"].as_array_mut() {
            content.extend(blocks);
            return;
        }
    }
    turns.push(json!({"role": role, "content": blocks}));
}

/// The arguments of canonical tool call `call`, when they are JSON.
pub(super) fn arguments(call: &Value) -> Option<Value> {
    serde_json::from_str(call["function"]["arguments"].as_str()?).ok()
}

/// A canonical tool call, from a native call's id, tool name and input.
pub(super) fn tool_call(id: &Value, name: &Value, input: &Value) -> Value {
    json!({
        "id": id,
        "type": "function",
        "function": {"name": name, "arguments": input.to_string()},
    })
}

/// A canonical assistant message: its `content` (text, or `null`), and its tool calls when it
/// makes any.
pub(super) fn assistant(content: Value, calls: Vec<Value>) -> Value {
    let mut m = json!({"role": "assistant", "content": content});
    if !calls.is_empty() {
        m["tool_calls"] = json!(calls);
    }
    m
}

/// The canonical answer of `model`: one choice, `message`, ended by `finish`, and its `usage`.
pub(super) fn completion(model: &Value, message: Value, finish: &str, usage: Value) -> Value {
    json!({
        "object": "chat.completion",
        "model": model,
        "choices": [{"index": 0, "message": message, "finish_reason": finish}],
        "usage": usage,
    })
}
