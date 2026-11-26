//! Search over every run's steps: the operation's input (JSON, or a query string where a list is
//! comma-separated), checked into a [`SearchQuery`] the store binds (`store/search.rs`).
//!
//! ```text
//!  q, kind, agent, run, experiment ──▶ SearchQuery ──▶ events_search (GIN, migration 8) ──▶ hits + snippets
//! ```
//!
//! A step is found by its text (`shakedown.step_text`, migration 8): reasoning, messages,
//! delegations, reports, hand-offs, a tool call's name, error, arguments and result, checks.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};

use super::filter::list;

/// The kinds of step search finds; every other kind has no searchable text.
pub const KINDS: [&str; 11] = [
    "reasoning",
    "message",
    "tool_call",
    "delegation",
    "report",
    "handoff",
    "wake",
    "idle",
    "finish",
    "agent_end",
    "check",
];

/// Where a snippet's match starts and stops (Unicode private-use characters, which step text
/// does not hold), so a client can mark it without parsing HTML.
pub const HIT_START: char = '\u{E000}';
pub const HIT_END: char = '\u{E001}';

/// Results per page unless asked for: and at most.
pub const DEFAULT_LIMIT: i64 = 50;
pub const MAX_LIMIT: i64 = 200;
/// How deep the pages go, and how far matches are counted.
pub const MAX_OFFSET: i64 = 10_000;

/// What to search for, and in which steps.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct SearchInput {
    /// The words to find, as in a web search: `crew swap`, `"exact phrase"`, `delay or cancel`,
    /// `-link`. Words match their other forms (crew finds crews).
    #[serde(default)]
    pub q: String,
    /// Kinds of step: reasoning, message, tool_call, delegation, report, handoff, wake, idle,
    /// finish, agent_end, check (comma-separated or a JSON array).
    #[serde(default, deserialize_with = "list")]
    #[schemars(with = "Option<String>")]
    pub kind: Vec<String>,
    /// Agents, by id.
    #[serde(default, deserialize_with = "list")]
    #[schemars(with = "Option<String>")]
    pub agent: Vec<String>,
    /// Runs, by id.
    #[serde(default, deserialize_with = "list")]
    #[schemars(with = "Option<String>")]
    pub run: Vec<String>,
    /// Experiments, by id: the runs they deployed.
    #[serde(default, deserialize_with = "list")]
    #[schemars(with = "Option<String>")]
    pub experiment: Vec<String>,
    /// Results per page (default 50, at most 200).
    #[serde(default, deserialize_with = "count")]
    #[schemars(with = "Option<i64>")]
    pub limit: Option<i64>,
    /// How many results to skip: the previous page's `next_offset`.
    #[serde(default, deserialize_with = "count")]
    #[schemars(with = "Option<i64>")]
    pub offset: Option<i64>,
}

/// A [`SearchInput`] checked and ready to bind.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchQuery {
    pub q: String,
    pub kind: Vec<String>,
    pub agent: Vec<String>,
    pub run: Vec<String>,
    pub experiment: Vec<String>,
    pub limit: i64,
    pub offset: i64,
}

impl SearchInput {
    /// The query of this input; no words, an unknown kind or a bad page is an error to show.
    pub fn resolve(self) -> Result<SearchQuery, String> {
        let q = self.q.trim().to_string();
        if q.is_empty() {
            return Err("q: give the words to search for".into());
        }
        if let Some(k) = self.kind.iter().find(|k| !KINDS.contains(&k.as_str())) {
            return Err(format!(
                "kind {k:?} is not searchable; one of {}",
                KINDS.join(", ")
            ));
        }
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(format!("limit is from 1 to {MAX_LIMIT}, not {limit}"));
        }
        let offset = self.offset.unwrap_or(0);
        if !(0..=MAX_OFFSET).contains(&offset) {
            return Err(format!("offset is from 0 to {MAX_OFFSET}, not {offset}"));
        }
        Ok(SearchQuery {
            q,
            kind: self.kind,
            agent: self.agent,
            run: self.run,
            experiment: self.experiment,
            limit,
            offset,
        })
    }
}

/// A whole number, as JSON or as query-string text.
fn count<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Count {
        Number(i64),
        Text(String),
    }
    match Option::<Count>::deserialize(d)? {
        None => Ok(None),
        Some(Count::Number(n)) => Ok(Some(n)),
        Some(Count::Text(t)) if t.trim().is_empty() => Ok(None),
        Some(Count::Text(t)) => {
            t.trim().parse().map(Some).map_err(|_| {
                serde::de::Error::custom(format!("expected a whole number, got {t:?}"))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resolve(v: serde_json::Value) -> Result<SearchQuery, String> {
        serde_json::from_value::<SearchInput>(v).unwrap().resolve()
    }

    #[test]
    fn lists_and_pages_come_as_json_or_query_text() {
        let q = resolve(
            json!({"q": " crew ", "kind": "reasoning,tool_call", "agent": ["ops"],
                                "limit": "20", "offset": 40}),
        )
        .unwrap();
        assert_eq!(q.q, "crew");
        assert_eq!(q.kind, ["reasoning", "tool_call"]);
        assert_eq!(q.agent, ["ops"]);
        assert_eq!((q.limit, q.offset), (20, 40));
        let d = resolve(json!({"q": "crew", "limit": ""})).unwrap();
        assert_eq!((d.limit, d.offset), (DEFAULT_LIMIT, 0));
    }

    #[test]
    fn no_words_unknown_kinds_and_bad_pages_are_refused() {
        assert!(resolve(json!({"q": "  "})).is_err());
        assert!(resolve(json!({"q": "x", "kind": "model_call"})).is_err());
        assert!(resolve(json!({"q": "x", "limit": 0})).is_err());
        assert!(resolve(json!({"q": "x", "limit": MAX_LIMIT + 1})).is_err());
        assert!(resolve(json!({"q": "x", "offset": -1})).is_err());
        assert!(serde_json::from_value::<SearchInput>(json!({"q": "x", "limit": "ten"})).is_err());
    }
}
