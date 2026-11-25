//! The OTLP messages shaked sends and receives (opentelemetry-proto v1, `trace` and `metrics`),
//! written out with `prost`'s derive: the subset in use, field numbers as in the `.proto` files,
//! so the bytes are OTLP/HTTP protobuf. No generated code, no gRPC.

use prost::{Message, Oneof};

// ------------------------------------------------------------------ common

#[derive(Clone, PartialEq, Message)]
pub struct KeyValue {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(message, optional, tag = "2")]
    pub value: Option<AnyValue>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AnyValue {
    #[prost(oneof = "Value", tags = "1, 2, 3, 4, 5, 6, 7")]
    pub value: Option<Value>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum Value {
    #[prost(string, tag = "1")]
    StringValue(String),
    #[prost(bool, tag = "2")]
    BoolValue(bool),
    #[prost(int64, tag = "3")]
    IntValue(i64),
    #[prost(double, tag = "4")]
    DoubleValue(f64),
    #[prost(message, tag = "5")]
    ArrayValue(ArrayValue),
    #[prost(message, tag = "6")]
    KvlistValue(KeyValueList),
    #[prost(bytes, tag = "7")]
    BytesValue(Vec<u8>),
}

#[derive(Clone, PartialEq, Message)]
pub struct ArrayValue {
    #[prost(message, repeated, tag = "1")]
    pub values: Vec<AnyValue>,
}

#[derive(Clone, PartialEq, Message)]
pub struct KeyValueList {
    #[prost(message, repeated, tag = "1")]
    pub values: Vec<KeyValue>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Resource {
    #[prost(message, repeated, tag = "1")]
    pub attributes: Vec<KeyValue>,
}

#[derive(Clone, PartialEq, Message)]
pub struct InstrumentationScope {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub version: String,
}

// ------------------------------------------------------------------ trace

#[derive(Clone, PartialEq, Message)]
pub struct ExportTraceServiceRequest {
    #[prost(message, repeated, tag = "1")]
    pub resource_spans: Vec<ResourceSpans>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ResourceSpans {
    #[prost(message, optional, tag = "1")]
    pub resource: Option<Resource>,
    #[prost(message, repeated, tag = "2")]
    pub scope_spans: Vec<ScopeSpans>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ScopeSpans {
    #[prost(message, optional, tag = "1")]
    pub scope: Option<InstrumentationScope>,
    #[prost(message, repeated, tag = "2")]
    pub spans: Vec<Span>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Span {
    #[prost(bytes, tag = "1")]
    pub trace_id: Vec<u8>,
    #[prost(bytes, tag = "2")]
    pub span_id: Vec<u8>,
    #[prost(bytes, tag = "4")]
    pub parent_span_id: Vec<u8>,
    #[prost(string, tag = "5")]
    pub name: String,
    /// 1 internal, 2 server, 3 client, 4 producer, 5 consumer.
    #[prost(int32, tag = "6")]
    pub kind: i32,
    #[prost(fixed64, tag = "7")]
    pub start_time_unix_nano: u64,
    #[prost(fixed64, tag = "8")]
    pub end_time_unix_nano: u64,
    #[prost(message, repeated, tag = "9")]
    pub attributes: Vec<KeyValue>,
    #[prost(message, repeated, tag = "11")]
    pub events: Vec<SpanEvent>,
    #[prost(message, optional, tag = "15")]
    pub status: Option<Status>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SpanEvent {
    #[prost(fixed64, tag = "1")]
    pub time_unix_nano: u64,
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(message, repeated, tag = "3")]
    pub attributes: Vec<KeyValue>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Status {
    #[prost(string, tag = "2")]
    pub message: String,
    /// 0 unset, 1 ok, 2 error.
    #[prost(int32, tag = "3")]
    pub code: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct ExportTraceServiceResponse {
    #[prost(message, optional, tag = "1")]
    pub partial_success: Option<PartialSuccess>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PartialSuccess {
    #[prost(int64, tag = "1")]
    pub rejected: i64,
    #[prost(string, tag = "2")]
    pub error_message: String,
}

// ------------------------------------------------------------------ metrics

#[derive(Clone, PartialEq, Message)]
pub struct ExportMetricsServiceRequest {
    #[prost(message, repeated, tag = "1")]
    pub resource_metrics: Vec<ResourceMetrics>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ResourceMetrics {
    #[prost(message, optional, tag = "1")]
    pub resource: Option<Resource>,
    #[prost(message, repeated, tag = "2")]
    pub scope_metrics: Vec<ScopeMetrics>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ScopeMetrics {
    #[prost(message, optional, tag = "1")]
    pub scope: Option<InstrumentationScope>,
    #[prost(message, repeated, tag = "2")]
    pub metrics: Vec<Metric>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Metric {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub description: String,
    #[prost(string, tag = "3")]
    pub unit: String,
    #[prost(oneof = "Data", tags = "5, 7, 9")]
    pub data: Option<Data>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum Data {
    #[prost(message, tag = "5")]
    Gauge(Gauge),
    #[prost(message, tag = "7")]
    Sum(Sum),
    #[prost(message, tag = "9")]
    Histogram(Histogram),
}

/// Cumulative: every point counts from the start (`start_time_unix_nano`).
pub const CUMULATIVE: i32 = 2;

#[derive(Clone, PartialEq, Message)]
pub struct Gauge {
    #[prost(message, repeated, tag = "1")]
    pub data_points: Vec<NumberDataPoint>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Sum {
    #[prost(message, repeated, tag = "1")]
    pub data_points: Vec<NumberDataPoint>,
    #[prost(int32, tag = "2")]
    pub aggregation_temporality: i32,
    #[prost(bool, tag = "3")]
    pub is_monotonic: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct Histogram {
    #[prost(message, repeated, tag = "1")]
    pub data_points: Vec<HistogramDataPoint>,
    #[prost(int32, tag = "2")]
    pub aggregation_temporality: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct NumberDataPoint {
    #[prost(message, repeated, tag = "7")]
    pub attributes: Vec<KeyValue>,
    #[prost(fixed64, tag = "2")]
    pub start_time_unix_nano: u64,
    #[prost(fixed64, tag = "3")]
    pub time_unix_nano: u64,
    /// Always set: a point without a value is refused by collectors.
    #[prost(double, optional, tag = "4")]
    pub as_double: Option<f64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct HistogramDataPoint {
    #[prost(message, repeated, tag = "9")]
    pub attributes: Vec<KeyValue>,
    #[prost(fixed64, tag = "2")]
    pub start_time_unix_nano: u64,
    #[prost(fixed64, tag = "3")]
    pub time_unix_nano: u64,
    #[prost(fixed64, tag = "4")]
    pub count: u64,
    #[prost(double, optional, tag = "5")]
    pub sum: Option<f64>,
    #[prost(fixed64, repeated, tag = "6")]
    pub bucket_counts: Vec<u64>,
    #[prost(double, repeated, tag = "7")]
    pub explicit_bounds: Vec<f64>,
}

/// The bytes of `m` as protobuf.
pub fn bytes_of(m: &impl Message) -> Vec<u8> {
    m.encode_to_vec()
}

fn any(v: Value) -> AnyValue {
    AnyValue { value: Some(v) }
}

/// An attribute's value as OTLP's.
pub fn any_value_of_attr(a: &super::span::Attr) -> AnyValue {
    use super::span::Attr;
    match a {
        Attr::Str(s) => any(Value::StringValue(s.clone())),
        Attr::Bool(b) => any(Value::BoolValue(*b)),
        Attr::Int(i) => any(Value::IntValue(*i)),
        Attr::Float(f) => any(Value::DoubleValue(*f)),
        Attr::List(items) => any(Value::ArrayValue(ArrayValue {
            values: items
                .iter()
                .map(|s| any(Value::StringValue(s.clone())))
                .collect(),
        })),
    }
}

/// OTLP's value as JSON: bytes in hex, a key-value list as an object.
pub fn json_of(v: &AnyValue) -> serde_json::Value {
    use serde_json::Value as J;
    match &v.value {
        None => J::Null,
        Some(Value::StringValue(s)) => J::String(s.clone()),
        Some(Value::BoolValue(b)) => J::Bool(*b),
        Some(Value::IntValue(i)) => J::from(*i),
        Some(Value::DoubleValue(f)) => serde_json::Number::from_f64(*f).map_or(J::Null, J::Number),
        Some(Value::ArrayValue(a)) => J::Array(a.values.iter().map(json_of).collect()),
        Some(Value::KvlistValue(kv)) => J::Object(object_of(&kv.values)),
        Some(Value::BytesValue(b)) => J::String(super::span::ids::to_hex(b)),
    }
}

/// Key-value pairs as a JSON object.
pub fn object_of(kv: &[KeyValue]) -> serde_json::Map<String, serde_json::Value> {
    let pairs = kv.iter().map(|p| {
        let v = p.value.as_ref().map_or(serde_json::Value::Null, json_of);
        (p.key.clone(), v)
    });
    pairs.collect()
}
