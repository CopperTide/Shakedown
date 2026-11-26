//! Failure names kept per run, never raw tool errors or customer records. Repeated refusals of
//! the same tool count once. Old records identify a refusal by the world's exact error prefix;
//! they do not identify a distinct gate, and integrations may use the same refusal convention.
use std::collections::BTreeSet;

use crate::model::{Event, EventKind};

/// Explicitly failed named checks in the final scoring event, with duplicates removed.
pub(super) fn failed_checks(events: &[Event]) -> Vec<String> {
    let Some(last) = events.iter().rev().find(|e| e.kind == EventKind::Check) else {
        return vec![];
    };
    last.payload
        .get("checks")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|c| c.get("passed").and_then(|v| v.as_bool()) == Some(false))
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Tools explicitly refused using the world's recorded `<tool> refused:` convention.
pub(super) fn refused_tools(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter(|e| e.kind == EventKind::ToolCall)
        .filter_map(|e| {
            let name = e.payload.get("name")?.as_str()?;
            let error = e.payload.get("error")?.as_str()?;
            (!name.trim().is_empty() && error.starts_with(&format!("{name} refused:")))
                .then(|| name.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;
    use serde_json::json;

    #[test]
    fn final_checks_only_explicit_failures_and_deduplicated_names() {
        let old = testkit::event(
            1,
            None,
            EventKind::Check,
            json!({"checks": [{"name":"old", "passed":false}]}),
        );
        let last = testkit::event(
            2,
            None,
            EventKind::Check,
            json!({"checks": [
                {"name":"safe", "passed":true}, {"name":"goal", "passed":false},
                {"name":"goal", "passed":false}, {"name":"unknown"}, {"name":" ", "passed":false}
            ]}),
        );
        assert_eq!(failed_checks(&[old, last]), ["goal"]);
        assert!(failed_checks(&[]).is_empty());
    }

    #[test]
    fn refusals_are_bounded_tool_names_not_error_text_or_ordinary_errors() {
        let refusal = |seq, name, error| {
            testkit::event(
                seq,
                Some("a"),
                EventKind::ToolCall,
                json!({"name":name, "error":error}),
            )
        };
        let events = [
            refusal(1, "book", "book refused: customer-secret"),
            refusal(2, "book", "book refused: another reason"),
            refusal(3, "read", "network refused connection"),
            refusal(4, "read", "book refused: wrong name"),
        ];
        assert_eq!(refused_tools(&events), ["book"]);
    }
}
