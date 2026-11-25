//! What an agent's OTLP exporter sends, as [`Imported`] spans: OTLP/HTTP protobuf (the SDKs'
//! default) or OTLP/JSON (hex ids, numbers as strings or numbers, camelCase), bounded per span
//! ([`MAX_ATTRS`], [`MAX_EVENTS`], text cut to [`crate::otel::span::MAX_TEXT`]).

use chrono::{DateTime, Utc};
use prost::Message;
use serde_json::{Map, Value};

use super::{
    imported::{Imported, ImportedEvent},
    proto,
    span::{cut, ids},
};

/// The most attributes kept of a span or event.
pub const MAX_ATTRS: usize = 128;
/// The most events kept of a span.
pub const MAX_EVENTS: usize = 64;

fn time(ns: u64) -> DateTime<Utc> {
    DateTime::from_timestamp_nanos(i64::try_from(ns).unwrap_or(i64::MAX))
}

/// `attributes` as a JSON object, bounded, text cut.
fn bounded(attributes: Map<String, Value>) -> Map<String, Value> {
    let cut_text = |v: Value| match v {
        Value::String(s) => Value::String(cut(&s)),
        other => other,
    };
    let kept = attributes.into_iter().take(MAX_ATTRS);
    kept.map(|(k, v)| (k, cut_text(v))).collect()
}

/// `attributes` with the resource's `service.name`, when it has one.
fn with_service(mut attributes: Map<String, Value>, service: Option<&Value>) -> Map<String, Value> {
    if let Some(svc) = service {
        attributes.insert("service.name".into(), svc.clone());
    }
    attributes
}

/// Why a span failed: its status message, else `error`.
fn failure(message: Option<&str>) -> String {
    message
        .filter(|m| !m.is_empty())
        .unwrap_or("error")
        .to_string()
}

/// One span of a protobuf request, with its resource's `service.name`.
fn from_proto(s: proto::Span, service: Option<&Value>) -> Imported {
    let attributes = with_service(bounded(proto::object_of(&s.attributes)), service);
    let events = s
        .events
        .into_iter()
        .take(MAX_EVENTS)
        .map(|e| ImportedEvent {
            name: e.name,
            at: time(e.time_unix_nano),
            attributes: bounded(proto::object_of(&e.attributes)),
        });
    let failed = s.status.filter(|st| st.code == 2);
    Imported {
        trace_id: ids::to_hex(&s.trace_id),
        span_id: ids::to_hex(&s.span_id),
        parent_span_id: (!s.parent_span_id.is_empty()).then(|| ids::to_hex(&s.parent_span_id)),
        name: cut(&s.name),
        kind: s.kind,
        start: time(s.start_time_unix_nano),
        end: time(s.end_time_unix_nano),
        attributes,
        events: events.collect(),
        error: failed.map(|st| failure(Some(&st.message))),
    }
}

/// The spans of a protobuf `ExportTraceServiceRequest`.
pub fn protobuf(body: &[u8]) -> Result<Vec<Imported>, String> {
    let req = proto::ExportTraceServiceRequest::decode(body)
        .map_err(|e| format!("not an OTLP protobuf trace request: {e}"))?;
    let mut out = vec![];
    for rs in req.resource_spans {
        let res = rs
            .resource
            .map(|r| proto::object_of(&r.attributes))
            .unwrap_or_default();
        for ss in rs.scope_spans {
            out.extend(
                ss.spans
                    .into_iter()
                    .map(|s| from_proto(s, res.get("service.name"))),
            );
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ OTLP/JSON

/// A number OTLP/JSON may write as a number or a string.
fn num(v: &Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

/// An OTLP/JSON `AnyValue` as plain JSON.
fn any_json(v: &Value) -> Value {
    let Some((k, inner)) = v.as_object().and_then(|o| o.iter().next()) else {
        return Value::Null;
    };
    match k.as_str() {
        "intValue" => inner
            .as_i64()
            .or_else(|| inner.as_str()?.parse().ok())
            .map_or(Value::Null, Value::from),
        "arrayValue" => Value::Array(
            inner["values"]
                .as_array()
                .into_iter()
                .flatten()
                .map(any_json)
                .collect(),
        ),
        "kvlistValue" => Value::Object(kv_json(&inner["values"])),
        _ => inner.clone(),
    }
}

fn kv_json(list: &Value) -> Map<String, Value> {
    let pairs = list.as_array().into_iter().flatten();
    let pairs = pairs.filter_map(|p| Some((p["key"].as_str()?.to_string(), any_json(&p["value"]))));
    bounded(pairs.collect())
}

/// OTLP/JSON's span kind or status code: a number, or its enum name.
fn enum_of(v: &Value, names: &[&str]) -> i32 {
    if let Some(n) = v.as_i64() {
        return i32::try_from(n).unwrap_or(0);
    }
    let name = v.as_str().unwrap_or_default();
    names
        .iter()
        .position(|n| name.ends_with(n))
        .map_or(0, |i| i as i32)
}

const KINDS: [&str; 6] = [
    "UNSPECIFIED",
    "INTERNAL",
    "SERVER",
    "CLIENT",
    "PRODUCER",
    "CONSUMER",
];
const CODES: [&str; 3] = ["UNSET", "OK", "ERROR"];

fn json_events(s: &Value) -> Vec<ImportedEvent> {
    let events = s["events"]
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_EVENTS);
    let event = |e: &Value| ImportedEvent {
        name: cut(e["name"].as_str().unwrap_or_default()),
        at: time(num(&e["timeUnixNano"])),
        attributes: kv_json(&e["attributes"]),
    };
    events.map(event).collect()
}

fn json_span(s: &Value, service: Option<&Value>) -> Imported {
    let failed = enum_of(&s["status"]["code"], &CODES) == 2;
    let hex = |k: &str| {
        s[k].as_str()
            .filter(|h| !h.is_empty())
            .map(str::to_ascii_lowercase)
    };
    Imported {
        trace_id: hex("traceId").unwrap_or_default(),
        span_id: hex("spanId").unwrap_or_default(),
        parent_span_id: hex("parentSpanId"),
        name: cut(s["name"].as_str().unwrap_or_default()),
        kind: enum_of(&s["kind"], &KINDS),
        start: time(num(&s["startTimeUnixNano"])),
        end: time(num(&s["endTimeUnixNano"])),
        attributes: with_service(kv_json(&s["attributes"]), service),
        events: json_events(s),
        error: failed.then(|| failure(s["status"]["message"].as_str())),
    }
}

/// The spans of an OTLP/JSON trace request.
pub fn json(body: &[u8]) -> Result<Vec<Imported>, String> {
    let v: Value = serde_json::from_slice(body).map_err(|e| format!("not OTLP/JSON: {e}"))?;
    let mut out = vec![];
    for rs in v["resourceSpans"].as_array().into_iter().flatten() {
        let res = kv_json(&rs["resource"]["attributes"]);
        let scopes = rs["scopeSpans"]
            .as_array()
            .or(rs["instrumentationLibrarySpans"].as_array());
        for ss in scopes.into_iter().flatten() {
            let spans = ss["spans"].as_array().into_iter().flatten();
            out.extend(spans.map(|s| json_span(s, res.get("service.name"))));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn otlp_json_reads_like_its_protobuf() {
        let body = json!({"resourceSpans": [{
            "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "my-agent"}}]},
            "scopeSpans": [{"spans": [{
                "traceId": "5B8EFFF798038103D269B633813FC60C", "spanId": "EEE19B7EC3C1B174",
                "parentSpanId": "", "name": "retrieve", "kind": "SPAN_KIND_CLIENT",
                "startTimeUnixNano": "1544712660000000000", "endTimeUnixNano": 1544712661000000000u64,
                "attributes": [{"key": "rows", "value": {"intValue": "3"}},
                               {"key": "tags", "value": {"arrayValue": {"values": [{"stringValue": "a"}]}}}],
                "events": [{"name": "cache miss", "timeUnixNano": "1544712660500000000"}],
                "status": {"code": 2, "message": "timed out"}}]}]}]});
        let spans = json(body.to_string().as_bytes()).unwrap();
        let s = &spans[0];
        assert_eq!(s.trace_id, "5b8efff798038103d269b633813fc60c");
        assert_eq!((s.parent_span_id.as_deref(), s.kind), (None, 3));
        assert_eq!(s.attributes["rows"], json!(3));
        assert_eq!(s.attributes["tags"], json!(["a"]));
        assert_eq!(s.attributes["service.name"], json!("my-agent"));
        assert_eq!(s.error.as_deref(), Some("timed out"));
        assert_eq!((s.end - s.start).num_seconds(), 1);
        assert_eq!(s.events[0].name, "cache miss");
        assert!(json(b"nope").is_err());
        assert!(protobuf(b"\xff\xff").is_err());
    }
}
