//! The world-time gateway: telemetry from wall to world time on the way to the stores (ingest),
//! and the stores' queries answered in world time (query). See README.md.
pub mod clock;
mod ingest;
mod query;
pub mod restamp;
mod shift;

use std::sync::Arc;
use std::time::Duration;

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;

use crate::clock::Clock;

/// How long a store may take to answer.
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(120);

/// Where each signal is sent and each store is asked.
#[derive(Debug, Clone)]
pub struct Upstreams {
    pub traces: String,
    pub logs: String,
    pub metrics: String,
    pub prometheus: String,
    pub loki: String,
    pub tempo: String,
}

pub struct Gateway {
    pub clock: Arc<Clock>,
    pub up: Upstreams,
    http: reqwest::Client,
}

impl Gateway {
    pub fn new(clock: Arc<Clock>, up: Upstreams) -> Gateway {
        let http = reqwest::Client::builder().timeout(UPSTREAM_TIMEOUT).build();
        Gateway {
            clock,
            up,
            http: http.expect("a plain HTTP client builds"),
        }
    }
}

/// The ingest face (`/v1/traces`, `/v1/logs`, `/v1/metrics`).
pub fn ingest_router(g: Arc<Gateway>) -> Router {
    ingest::router(g)
}

/// The query face (`/q/<store>/...`, or a store's host at the root).
pub fn query_router(g: Arc<Gateway>) -> Router {
    query::router(g)
}

/// The store's answer (status, content type and the body `body` makes of its body), or 502
/// when it can't be reached.
async fn upstream_answer(
    sent: reqwest::Result<reqwest::Response>,
    body: impl FnOnce(Vec<u8>) -> Vec<u8>,
) -> Response {
    let answer = match sent {
        Ok(a) => a,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("the store can't be reached: {e}"),
            )
                .into_response()
        }
    };
    let status = StatusCode::from_u16(answer.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let ct = answer.headers().get(header::CONTENT_TYPE).cloned();
    let bytes = match answer.bytes().await {
        Ok(b) => body(b.to_vec()),
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("the store's answer broke off: {e}"),
            )
                .into_response()
        }
    };
    let mut r = (status, bytes).into_response();
    if let Some(ct) = ct {
        r.headers_mut().insert(header::CONTENT_TYPE, ct);
    }
    r
}
