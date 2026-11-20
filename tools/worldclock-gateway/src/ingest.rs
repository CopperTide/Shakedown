//! The ingest face: `POST /v1/traces`, `/v1/logs`, `/v1/metrics` in OTLP/HTTP, protobuf or JSON
//! (uncompressed), every time restamped to world time and sent on in the same encoding to the
//! signal's store, whose answer (status, type, body) goes back to the sender.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use prost::Message;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::clock::Clock;
use crate::{restamp, upstream_answer, Gateway};

/// The largest export taken (the collector batches 512 items).
const MAX_BODY: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
enum Signal {
    Traces,
    Logs,
    Metrics,
}

pub fn router(g: Arc<Gateway>) -> Router {
    let face =
        |s: Signal| move |st: State<Arc<Gateway>>, h: HeaderMap, b: Bytes| ingest(st, s, h, b);
    Router::new()
        .route("/v1/traces", post(face(Signal::Traces)))
        .route("/v1/logs", post(face(Signal::Logs)))
        .route("/v1/metrics", post(face(Signal::Metrics)))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(g)
}

/// A 415 for a compressed export; None for an uncompressed one.
fn compressed(h: &HeaderMap) -> Option<Response> {
    let encoding = h.get(header::CONTENT_ENCODING)?.to_str().ok()?;
    let why = "compressed exports are not taken: send them uncompressed";
    (encoding != "identity").then(|| (StatusCode::UNSUPPORTED_MEDIA_TYPE, why).into_response())
}

impl Signal {
    fn url(self, g: &Gateway) -> &str {
        match self {
            Signal::Traces => &g.up.traces,
            Signal::Logs => &g.up.logs,
            Signal::Metrics => &g.up.metrics,
        }
    }
}

async fn ingest(State(g): State<Arc<Gateway>>, s: Signal, h: HeaderMap, body: Bytes) -> Response {
    if let Some(refused) = compressed(&h) {
        return refused;
    }
    let content_type = h.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok());
    let json = content_type.is_some_and(|t| t.starts_with("application/json"));
    let restamped = match restamp_body(s, &body, json, &g.clock) {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    let ct = match json {
        true => "application/json",
        false => "application/x-protobuf",
    };
    let sent = g.http.post(s.url(&g)).header(header::CONTENT_TYPE, ct);
    upstream_answer(sent.body(restamped).send().await, |b| b).await
}

/// The export in `body`, every time in world time, encoded as it came.
fn restamp_body(s: Signal, body: &[u8], json: bool, c: &Clock) -> Result<Vec<u8>, String> {
    match s {
        Signal::Traces => {
            let mut r: ExportTraceServiceRequest = decode(body, json)?;
            restamp::traces(&mut r, c);
            encode(&r, json)
        }
        Signal::Logs => {
            let mut r: ExportLogsServiceRequest = decode(body, json)?;
            restamp::logs(&mut r, c);
            encode(&r, json)
        }
        Signal::Metrics => {
            let mut r: ExportMetricsServiceRequest = decode(body, json)?;
            restamp::metrics(&mut r, c);
            encode(&r, json)
        }
    }
}

fn decode<T: Message + Default + DeserializeOwned>(body: &[u8], json: bool) -> Result<T, String> {
    if json {
        serde_json::from_slice(body).map_err(|e| format!("not OTLP JSON: {e}"))
    } else {
        T::decode(body).map_err(|e| format!("not OTLP protobuf: {e}"))
    }
}

fn encode<T: Message + Serialize>(r: &T, json: bool) -> Result<Vec<u8>, String> {
    if json {
        serde_json::to_vec(r).map_err(|e| e.to_string())
    } else {
        Ok(r.encode_to_vec())
    }
}
