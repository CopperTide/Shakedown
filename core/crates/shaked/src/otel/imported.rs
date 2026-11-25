//! Spans an agent's own tracing sent during its run, as a `spans` event keeps them ([`Imported`]),
//! and back out as spans of the run's trace: each keeps its place in the agent's tree, and the
//! roots of that tree hang under the agent's turn ([`Remap`]).

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{
    span::{cut, ids, is_content, Attr, Attrs, Failed, Kind, SpanEvent, SpanOut, MAX_LIST},
    steps::Ctx,
};
use crate::model::Event;

/// One span as the agent sent it, ids in hex.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Imported {
    pub trace_id: String,
    pub span_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub name: String,
    /// OTLP's span kind (1 internal, 2 server, 3 client, …).
    #[serde(default)]
    pub kind: i32,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub attributes: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<ImportedEvent>,
    /// Its status message when it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportedEvent {
    pub name: String,
    pub at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub attributes: Map<String, Value>,
}

/// The payload of a `spans` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SpansPayload {
    pub spans: Vec<Imported>,
}

/// The agents' spans of a branch, by their `(trace, span)` ids: what their parents become.
pub struct Remap {
    branch: String,
    known: HashSet<(String, String)>,
}

fn decoded(e: &Event) -> SpansPayload {
    serde_json::from_value(e.payload.clone()).unwrap_or_default()
}

impl Remap {
    /// The spans of `events`' `spans` events, on `branch`.
    pub fn of(branch: &str, events: &[&Event]) -> Remap {
        let spans = events
            .iter()
            .filter(|e| e.kind == crate::model::EventKind::Spans)
            .flat_map(|e| decoded(e).spans);
        let known = spans.map(|s| (s.trace_id, s.span_id)).collect();
        Remap {
            branch: branch.to_string(),
            known,
        }
    }

    fn id(&self, trace: &str, span: &str) -> String {
        ids::span(&self.branch, &format!("imp:{trace}:{span}"))
    }

    /// The spans of event `e`, their roots under `turn`.
    pub fn spans(&self, c: &Ctx, e: &Event, turn: &str) -> Vec<SpanOut> {
        let parent_of = |s: &Imported| match &s.parent_span_id {
            Some(p) if self.known.contains(&(s.trace_id.clone(), p.clone())) => {
                self.id(&s.trace_id, p)
            }
            _ => turn.to_string(),
        };
        let out = decoded(e).spans.into_iter().map(|s| SpanOut {
            trace_id: c.trace.clone(),
            span_id: self.id(&s.trace_id, &s.span_id),
            parent: Some(parent_of(&s)),
            name: s.name.clone(),
            kind: match s.kind {
                3 => Kind::Client,
                _ => Kind::Internal,
            },
            start: s.start,
            end: s.end.max(s.start),
            attrs: imported_attrs(&s, c.content),
            events: s.events.iter().map(|ev| event_of(ev, c.content)).collect(),
            error: s.error.clone().map(|message| Failed { message }),
        });
        out.collect()
    }
}

/// The text attributes of an agent's spans that are kept without content: what names the step,
/// never what it read or wrote. Numbers and yes-or-no values are kept too.
const STRUCTURAL: [&str; 16] = [
    "service.name",
    "gen_ai.operation.name",
    "gen_ai.system",
    "gen_ai.provider.name",
    "gen_ai.request.model",
    "gen_ai.response.model",
    "gen_ai.response.finish_reasons",
    "gen_ai.agent.name",
    "gen_ai.tool.name",
    "openinference.span.kind",
    "llm.model_name",
    "llm.provider",
    "llm.system",
    "tool.name",
    "http.request.method",
    "error.type",
];

/// Whether attribute `k` of value `v` goes out without content.
fn structural(k: &str, v: &Value) -> bool {
    match v {
        Value::Number(_) | Value::Bool(_) => !is_content(k),
        _ => STRUCTURAL.contains(&k),
    }
}

/// `attributes` as span attributes; without `content`, only [`structural`] ones.
fn attrs_of(attributes: &Map<String, Value>, content: bool) -> Attrs {
    let kept = attributes
        .iter()
        .filter(|(k, v)| content || structural(k, v));
    kept.filter_map(|(k, v)| Some((k.clone(), attr_of(v)?)))
        .collect()
}

fn imported_attrs(s: &Imported, content: bool) -> Attrs {
    let mut a = attrs_of(&s.attributes, content);
    a.insert("shakedown.imported".into(), Attr::Bool(true));
    a.insert(
        "shakedown.imported.trace_id".into(),
        Attr::Str(s.trace_id.clone()),
    );
    a.insert(
        "shakedown.imported.span_id".into(),
        Attr::Str(s.span_id.clone()),
    );
    a
}

fn event_of(ev: &ImportedEvent, content: bool) -> SpanEvent {
    SpanEvent {
        name: ev.name.clone(),
        at: ev.at,
        attrs: attrs_of(&ev.attributes, content),
    }
}

/// A JSON value as an attribute: scalars as they are, a list as text items, an object as JSON.
pub fn attr_of(v: &Value) -> Option<Attr> {
    Some(match v {
        Value::Null => return None,
        Value::Bool(b) => Attr::Bool(*b),
        Value::Number(n) if n.is_i64() => Attr::Int(n.as_i64()?),
        Value::Number(n) => Attr::Float(n.as_f64()?),
        Value::String(s) => Attr::Str(cut(s)),
        Value::Array(items) => {
            let text = |i: &Value| i.as_str().map_or_else(|| i.to_string(), str::to_string);
            Attr::List(items.iter().take(MAX_LIST).map(text).collect())
        }
        Value::Object(_) => Attr::Str(cut(&v.to_string())),
    })
}
