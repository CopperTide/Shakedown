//! Spans as OTLP/HTTP protobuf requests: one resource (`service.name` shakedown and, for Arize
//! Phoenix, the project the spans go to), one scope, at most [`BATCH`] spans a request.

use chrono::{DateTime, Utc};

use super::{
    proto::{self, any_value_of_attr, bytes_of},
    span::{ids, Attrs, Kind, SpanOut},
};

/// The most spans one request carries.
pub const BATCH: usize = 256;

/// What the instrumentation scope is called.
pub const SCOPE: &str = "shakedown";

/// Nanoseconds since the epoch (0 before it).
pub fn nanos(t: DateTime<Utc>) -> u64 {
    t.timestamp_nanos_opt().map_or(0, |n| n.max(0) as u64)
}

pub fn key_values(a: &Attrs) -> Vec<proto::KeyValue> {
    let kv = a.iter().map(|(k, v)| proto::KeyValue {
        key: k.clone(),
        value: Some(any_value_of_attr(v)),
    });
    kv.collect()
}

fn span_of(s: &SpanOut) -> proto::Span {
    let events = s.events.iter().map(|e| proto::SpanEvent {
        time_unix_nano: nanos(e.at),
        name: e.name.clone(),
        attributes: key_values(&e.attrs),
    });
    proto::Span {
        trace_id: ids::bytes(&s.trace_id),
        span_id: ids::bytes(&s.span_id),
        parent_span_id: s.parent.as_deref().map(ids::bytes).unwrap_or_default(),
        name: s.name.clone(),
        kind: match s.kind {
            Kind::Internal => 1,
            Kind::Client => 3,
        },
        start_time_unix_nano: nanos(s.start),
        end_time_unix_nano: nanos(s.end.max(s.start)),
        attributes: key_values(&s.attrs),
        events: events.collect(),
        status: s.error.as_ref().map(|f| proto::Status {
            message: f.message.clone(),
            code: 2,
        }),
    }
}

/// The resource spans are sent as: shakedown, and Phoenix's project when one is named.
pub fn resource(project: Option<&str>) -> proto::Resource {
    let mut a = Attrs::new();
    let s = |v: &str| super::span::Attr::Str(v.to_string());
    a.insert("service.name".into(), s("shakedown"));
    a.insert("service.version".into(), s(env!("CARGO_PKG_VERSION")));
    a.insert("telemetry.sdk.name".into(), s("shakedown"));
    if let Some(p) = project.filter(|p| !p.is_empty()) {
        a.insert("openinference.project.name".into(), s(p));
    }
    proto::Resource {
        attributes: key_values(&a),
    }
}

pub fn scope() -> proto::InstrumentationScope {
    proto::InstrumentationScope {
        name: SCOPE.into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

/// `spans` as request bodies of at most [`BATCH`] spans each.
pub fn requests(spans: &[SpanOut], project: Option<&str>) -> Vec<Vec<u8>> {
    let request = |chunk: &[SpanOut]| proto::ExportTraceServiceRequest {
        resource_spans: vec![proto::ResourceSpans {
            resource: Some(resource(project)),
            scope_spans: vec![proto::ScopeSpans {
                scope: Some(scope()),
                spans: chunk.iter().map(span_of).collect(),
            }],
        }],
    };
    spans.chunks(BATCH).map(|c| bytes_of(&request(c))).collect()
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::*;
    use crate::otel::span::{Attr, Failed};

    fn a_span() -> SpanOut {
        let at = DateTime::from_timestamp(1_700_000_000, 5).unwrap();
        let mut attrs = Attrs::new();
        attrs.insert("gen_ai.usage.input_tokens".into(), Attr::Int(12));
        SpanOut {
            trace_id: "0123456789abcdef0123456789abcdef".into(),
            span_id: "0123456789abcdef".into(),
            parent: Some("fedcba9876543210".into()),
            name: "chat m".into(),
            kind: Kind::Client,
            start: at,
            end: at,
            attrs,
            events: vec![],
            error: Some(Failed {
                message: "no".into(),
            }),
        }
    }

    #[test]
    fn spans_encode_as_otlp_protobuf_and_decode_back() {
        let bodies = requests(&vec![a_span(); BATCH + 1], Some("proj"));
        assert_eq!(bodies.len(), 2, "batched");
        let r = proto::ExportTraceServiceRequest::decode(&bodies[1][..]).unwrap();
        let span = &r.resource_spans[0].scope_spans[0].spans[0];
        assert_eq!(span.trace_id.len(), 16);
        assert_eq!(span.parent_span_id, ids::bytes("fedcba9876543210"));
        assert_eq!(span.kind, 3);
        assert_eq!(span.start_time_unix_nano, 1_700_000_000_000_000_005);
        assert_eq!(span.status.as_ref().unwrap().code, 2);
        let res = r.resource_spans[0].resource.as_ref().unwrap();
        assert!(res
            .attributes
            .iter()
            .any(|kv| kv.key == "openinference.project.name"));
    }
}
