//! What a passed-through call answered, read for its record: a streamed answer (server-sent
//! events) made whole again, and the usage of either API in the canonical token counts.

use serde_json::{json, Value};

use super::Api;
use crate::providers::canonical::cached_usage;

/// The `type`s of the event that ends a Responses stream with the whole response.
const FINAL: [&str; 3] = [
    "response.completed",
    "response.incomplete",
    "response.failed",
];

/// The data of each event of the stream `text` that is JSON (`[DONE]` is not).
pub(super) fn events(text: &str) -> Vec<Value> {
    let text = text.replace("\r\n", "\n");
    let data = |block: &str| {
        let lines = block.lines().filter_map(|l| l.strip_prefix("data:"));
        let lines: Vec<&str> = lines.map(|d| d.strip_prefix(' ').unwrap_or(d)).collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    };
    let blocks = text.split("\n\n").filter_map(data);
    blocks
        .filter_map(|d| serde_json::from_str(&d).ok())
        .collect()
}

/// Why the stream `text` of an `api` call stopped short when it did: the provider said `error` in
/// it, or it ended without its final event (`message_stop`, or a `message_delta` with its stop
/// reason, for Messages; `response.completed` or `response.incomplete` for Responses).
pub(super) fn cut_short(api: Api, text: &str) -> Option<String> {
    let events = events(text);
    let kind = |e: &Value| e["type"].as_str().unwrap_or_default().to_string();
    if let Some(e) = events.iter().find(|e| e["type"] == "error") {
        let said = e["error"]["message"].as_str().unwrap_or("no message");
        return Some(format!("the provider's stream said error: {said}"));
    }
    let ended = match api {
        Api::Messages => events.iter().any(|e| {
            kind(e) == "message_stop"
                || (kind(e) == "message_delta" && !e["delta"]["stop_reason"].is_null())
        }),
        Api::Responses => (events.iter())
            .any(|e| ["response.completed", "response.incomplete"].contains(&kind(e).as_str())),
        Api::Chat | Api::CountTokens => true,
    };
    (!ended).then(|| "the provider's stream ended without its final event".to_string())
}

/// The answer the stream `text` of an `api` call gave, whole: a Responses stream's final
/// response, the message a Messages stream's events build; else `{events}`, each event's data.
pub(super) fn assembled(api: Api, text: &str) -> Value {
    let events = events(text);
    let whole = match api {
        Api::Responses => (events.iter().rev())
            .find(|e| e["type"].as_str().is_some_and(|t| FINAL.contains(&t)))
            .map(|e| e["response"].clone()),
        Api::Messages => message(&events),
        Api::Chat | Api::CountTokens => None,
    };
    whole.unwrap_or_else(|| json!({ "events": events }))
}

/// The message a Messages stream's `events` build: `message_start`'s, each content block with
/// its deltas, and `message_delta`'s stop reason and usage.
fn message(events: &[Value]) -> Option<Value> {
    let start = events.iter().find(|e| e["type"] == "message_start")?;
    let mut msg = start["message"].clone();
    if !msg["content"].is_array() {
        msg["content"] = json!([]);
    }
    for e in events {
        apply(&mut msg, e);
    }
    Some(msg)
}

fn apply(msg: &mut Value, e: &Value) {
    let at = e["index"].as_u64().unwrap_or(0) as usize;
    match e["type"].as_str() {
        Some("content_block_start") => {
            if let Some(blocks) = msg["content"].as_array_mut() {
                blocks.push(e["content_block"].clone());
            }
        }
        Some("content_block_delta") => {
            if let Some(block) = msg["content"].get_mut(at) {
                delta(block, &e["delta"]);
            }
        }
        Some("content_block_stop") => {
            if let Some(block) = msg["content"].get_mut(at) {
                tool_input(block);
            }
        }
        Some("message_delta") => {
            merge(msg, &e["delta"]);
            merge(&mut msg["usage"], &e["usage"]);
        }
        _ => {}
    }
}

/// Copies every field of the object `from` into `into`.
fn merge(into: &mut Value, from: &Value) {
    let Some(from) = from.as_object() else {
        return;
    };
    if !into.is_object() {
        *into = json!({});
    }
    for (k, v) in from {
        into[k] = v.clone();
    }
}

/// `block` with one delta applied: text, thinking, a signature, or a piece of a tool's input.
fn delta(block: &mut Value, d: &Value) {
    let (key, piece) = match d["type"].as_str() {
        Some("text_delta") => ("text", &d["text"]),
        Some("thinking_delta") => ("thinking", &d["thinking"]),
        Some("input_json_delta") => ("partial_json", &d["partial_json"]),
        Some("signature_delta") => {
            block["signature"] = d["signature"].clone();
            return;
        }
        _ => return,
    };
    let so_far = block[key].as_str().unwrap_or_default();
    block[key] = json!(format!("{so_far}{}", piece.as_str().unwrap_or_default()));
}

/// A tool-use block's input, once its pieces have all come.
fn tool_input(block: &mut Value) {
    let Some(obj) = block.as_object_mut() else {
        return;
    };
    if let Some(Value::String(text)) = obj.remove("partial_json") {
        let input = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
        obj.insert("input".into(), input);
    }
}

/// The usage of an `api` answer in canonical counts (`prompt_tokens` the whole input), with
/// the API's own counts of cached input, and the cost when the provider reports one.
pub(super) fn usage(api: Api, response: &Value) -> Value {
    let u = &response["usage"];
    let n = |k: &str| u[k].as_i64().unwrap_or(0);
    let mut out = match api {
        Api::Messages => {
            let (fresh, written) = (n("input_tokens"), n("cache_creation_input_tokens"));
            let (read, output) = (n("cache_read_input_tokens"), n("output_tokens"));
            cached_usage(fresh, written, read, output)
        }
        Api::Responses | Api::Chat | Api::CountTokens => {
            let (input, output) = (n("input_tokens"), n("output_tokens"));
            let cached = u["input_tokens_details"]["cached_tokens"].as_i64();
            json!({"prompt_tokens": input, "completion_tokens": output,
                   "total_tokens": input + output,
                   "prompt_tokens_details": {"cached_tokens": cached.unwrap_or(0)}})
        }
    };
    if let Some(cost) = u.get("cost") {
        out["cost"] = cost.clone();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sse(events: &[Value]) -> String {
        let one = |e: &Value| {
            format!(
                "event: {}\r\ndata: {e}\r\n\r\n",
                e["type"].as_str().unwrap()
            )
        };
        events.iter().map(one).collect()
    }

    #[test]
    fn a_messages_stream_builds_its_message() {
        let text = sse(&[
            json!({"type": "message_start", "message": {"id": "m", "content": [],
                   "usage": {"input_tokens": 3, "output_tokens": 1}}}),
            json!({"type": "content_block_start", "index": 0,
                   "content_block": {"type": "text", "text": ""}}),
            json!({"type": "content_block_delta", "index": 0,
                   "delta": {"type": "text_delta", "text": "he"}}),
            json!({"type": "content_block_delta", "index": 0,
                   "delta": {"type": "text_delta", "text": "llo"}}),
            json!({"type": "content_block_start", "index": 1,
                   "content_block": {"type": "tool_use", "id": "t", "name": "f", "input": {}}}),
            json!({"type": "content_block_delta", "index": 1,
                   "delta": {"type": "input_json_delta", "partial_json": "{\"a\":"}}),
            json!({"type": "content_block_delta", "index": 1,
                   "delta": {"type": "input_json_delta", "partial_json": "1}"}}),
            json!({"type": "content_block_stop", "index": 1}),
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"},
                   "usage": {"output_tokens": 9}}),
            json!({"type": "message_stop"}),
        ]);
        let m = assembled(Api::Messages, &format!("{text}data: [DONE]\n\n"));
        assert_eq!(m["content"][0], json!({"type": "text", "text": "hello"}));
        let tool = json!({"type": "tool_use", "id": "t", "name": "f", "input": {"a": 1}});
        assert_eq!(m["content"][1], tool);
        assert_eq!(m["stop_reason"], "tool_use");
        assert_eq!(m["usage"], json!({"input_tokens": 3, "output_tokens": 9}));
        assert_eq!(usage(Api::Messages, &m)["total_tokens"], 12);
    }

    #[test]
    fn a_stream_without_its_final_event_or_with_an_error_is_cut_short() {
        let created = json!({"type": "response.created", "response": {"id": "r"}});
        let completed = json!({"type": "response.completed", "response": {"id": "r"}});
        assert!(cut_short(Api::Responses, &sse(std::slice::from_ref(&created))).is_some());
        assert_eq!(cut_short(Api::Responses, &sse(&[created, completed])), None);
        let start = json!({"type": "message_start", "message": {"content": []}});
        let delta = json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}});
        let error = json!({"type": "error", "error": {"type": "overloaded_error",
                                                       "message": "Overloaded"}});
        assert!(cut_short(Api::Messages, &sse(std::slice::from_ref(&start))).is_some());
        assert_eq!(
            cut_short(Api::Messages, &sse(&[start.clone(), delta])),
            None
        );
        let said = cut_short(Api::Messages, &sse(&[start, error])).unwrap();
        assert!(said.contains("Overloaded"), "{said}");
    }

    #[test]
    fn a_stream_without_its_end_keeps_its_events() {
        let text = sse(&[json!({"type": "response.created", "response": {"id": "r"}})]);
        let got = assembled(Api::Responses, &text);
        assert_eq!(got["events"][0]["type"], "response.created");
    }

    #[test]
    fn usage_is_canonical_with_the_apis_own_counts() {
        let r = json!({"usage": {"input_tokens": 20, "output_tokens": 7, "cost": 0.5,
                                 "input_tokens_details": {"cached_tokens": 4}}});
        let u = usage(Api::Responses, &r);
        assert_eq!(
            (&u["prompt_tokens"], &u["total_tokens"]),
            (&json!(20), &json!(27))
        );
        assert_eq!(
            (&u["prompt_tokens_details"]["cached_tokens"], &u["cost"]),
            (&json!(4), &json!(0.5))
        );
    }
}
