//! A span as shaked builds it from a run's record, before it is encoded: ids in hex, times as
//! timestamps, attributes in a sorted map (so a recorded run maps to the same JSON every time,
//! `tests/fixtures/otel/`). Ids are derived from the run's and branch's ids, so exporting a run
//! again sends the same spans ([`ids`]).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

/// The longest text value a span carries; longer ones are cut and marked.
pub const MAX_TEXT: usize = 32 * 1024;
/// The most items a list attribute carries.
pub const MAX_LIST: usize = 50;

/// An attribute's value.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Attr {
    Str(String),
    Bool(bool),
    Int(i64),
    Float(f64),
    List(Vec<String>),
}

/// A span's attributes by key.
pub type Attrs = BTreeMap<String, Attr>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Internal,
    Client,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpanEvent {
    pub name: String,
    pub at: DateTime<Utc>,
    pub attrs: Attrs,
}

/// A failed span says why; a span that worked has no status (unset).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Failed {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpanOut {
    pub trace_id: String,
    pub span_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub name: String,
    pub kind: Kind,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub attrs: Attrs,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<SpanEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Failed>,
}

/// `text` cut to [`MAX_TEXT`] bytes (on a character boundary), marked when cut.
pub fn cut(text: &str) -> String {
    if text.len() <= MAX_TEXT {
        return text.to_string();
    }
    let mut end = MAX_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[cut]", &text[..end])
}

/// `v` as text: a string as it is, anything else as JSON; cut.
pub fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => cut(s),
        other => cut(&other.to_string()),
    }
}

/// Builds attributes, leaving out what is absent.
#[derive(Debug, Default)]
pub struct AttrsBuilder(pub Attrs);

impl AttrsBuilder {
    pub fn str(mut self, k: &str, v: impl Into<String>) -> Self {
        self.0.insert(k.to_string(), Attr::Str(cut(&v.into())));
        self
    }

    pub fn opt_str(self, k: &str, v: Option<&str>) -> Self {
        match v.filter(|s| !s.is_empty()) {
            Some(s) => self.str(k, s),
            None => self,
        }
    }

    pub fn int(mut self, k: &str, v: i64) -> Self {
        self.0.insert(k.to_string(), Attr::Int(v));
        self
    }

    pub fn opt_int(self, k: &str, v: Option<i64>) -> Self {
        match v {
            Some(n) => self.int(k, n),
            None => self,
        }
    }

    pub fn opt_float(mut self, k: &str, v: Option<f64>) -> Self {
        if let Some(f) = v.filter(|f| f.is_finite()) {
            self.0.insert(k.to_string(), Attr::Float(f));
        }
        self
    }

    pub fn bool(mut self, k: &str, v: bool) -> Self {
        self.0.insert(k.to_string(), Attr::Bool(v));
        self
    }

    pub fn list(mut self, k: &str, v: impl IntoIterator<Item = String>) -> Self {
        let items: Vec<String> = v.into_iter().take(MAX_LIST).collect();
        if !items.is_empty() {
            self.0.insert(k.to_string(), Attr::List(items));
        }
        self
    }

    /// The content attributes, only when `content` (prompts, completions, tool arguments).
    pub fn content(self, content: bool, f: impl FnOnce(Self) -> Self) -> Self {
        match content {
            true => f(self),
            false => self,
        }
    }

    pub fn build(self) -> Attrs {
        self.0
    }
}

/// The attribute keys that hold content (what an agent read or wrote, prompts, completions),
/// dropped from imported spans when a target leaves content out. Keys starting with one of these.
pub const CONTENT_KEYS: [&str; 12] = [
    "input.value",
    "output.value",
    "llm.input_messages",
    "llm.output_messages",
    "llm.prompts",
    "tool.parameters",
    "gen_ai.prompt",
    "gen_ai.completion",
    "gen_ai.input.messages",
    "gen_ai.output.messages",
    "gen_ai.tool.call.arguments",
    "gen_ai.tool.call.result",
];

/// Whether attribute `key` holds content.
pub fn is_content(key: &str) -> bool {
    CONTENT_KEYS.iter().any(|k| key.starts_with(k)) || key.starts_with("shakedown.content.")
}

/// Deterministic trace and span ids: a run's trace id is its id when that is 32 hex digits
/// (shaked's run ids are), else the first 16 bytes of its SHA-256; a span id is the first 8 bytes
/// of the SHA-256 of `<scope>:<key>`.
pub mod ids {
    fn sha256(s: &str) -> [u8; 32] {
        let d = ring::digest::digest(&ring::digest::SHA256, s.as_bytes());
        d.as_ref().try_into().expect("SHA-256 is 32 bytes")
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The trace id of run `run`, in hex.
    pub fn trace(run: &str) -> String {
        let plain = run.len() == 32 && run.bytes().all(|b| b.is_ascii_hexdigit());
        match plain {
            true => run.to_ascii_lowercase(),
            false => hex(&sha256(run)[..16]),
        }
    }

    /// The id of span `key` of `scope` (a run or branch id), in hex.
    pub fn span(scope: &str, key: &str) -> String {
        hex(&sha256(&format!("{scope}:{key}"))[..8])
    }

    /// `hex` as bytes (empty when it isn't hex).
    pub fn bytes(hex: &str) -> Vec<u8> {
        let pairs = (0..hex.len()).step_by(2);
        let out: Option<Vec<u8>> = pairs
            .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
            .collect();
        out.unwrap_or_default()
    }

    /// `bytes` in hex.
    pub fn to_hex(bytes: &[u8]) -> String {
        hex(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_shaped_like_otlp_s() {
        let run = "0123456789abcdef0123456789abcdef";
        assert_eq!(ids::trace(run), run);
        assert_eq!(ids::trace("run-1").len(), 32);
        let a = ids::span("b1", "seq:3");
        assert_eq!(a.len(), 16);
        assert_eq!(a, ids::span("b1", "seq:3"));
        assert_ne!(a, ids::span("b2", "seq:3"));
        assert_eq!(ids::to_hex(&ids::bytes(&a)), a);
        assert!(ids::bytes("zz").is_empty());
    }

    #[test]
    fn long_text_is_cut_on_a_character_boundary() {
        let long = "é".repeat(MAX_TEXT);
        let c = cut(&long);
        assert!(c.ends_with("…[cut]"));
        assert!(c.len() <= MAX_TEXT + "…[cut]".len());
        assert_eq!(cut("short"), "short");
    }

    #[test]
    fn content_keys_are_recognised() {
        assert!(is_content("input.value"));
        assert!(is_content("llm.input_messages.0.message.content"));
        assert!(!is_content("gen_ai.request.model"));
    }
}
