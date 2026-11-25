//! The OTLP/HTTP receiver of an agent's own spans, `/otlp/{branch}/{agent}/v1/traces`: a live
//! branch's agent, with its token, sends what its tracing saw, and each request becomes one
//! `spans` event of that agent (so the agent's internal steps sit in the record beside its
//! model and tool calls, and go out with the run's trace). Standard OTel SDKs find it through
//! `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_EXPORTER_OTLP_HEADERS` ([`env`]). Bounded: a body of at
//! most [`MAX_BODY`], [`MAX_SPANS`] spans a request, [`BRANCH_SPANS`] a branch, [`PER_SECOND`]
//! requests a second a branch. `/v1/metrics` and `/v1/logs` are taken and dropped, so an SDK that
//! sends every signal there does not fail.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};

use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use prost::Message;

use super::{
    imported::{Imported, SpansPayload},
    inbound, proto,
};
use crate::{app::AppState, model::EventKind};

pub const MAX_BODY: usize = 4 << 20;
pub const MAX_SPANS: usize = 512;
pub const BRANCH_SPANS: usize = 20_000;
pub const PER_SECOND: u32 = 20;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/otlp/{branch}/{agent}/v1/traces", post(traces))
        .route("/otlp/{branch}/{agent}/v1/metrics", post(dropped))
        .route("/otlp/{branch}/{agent}/v1/logs", post(dropped))
}

/// The variables that point an OTel SDK at `agent`'s receiver at `base` (its `otlp_url`), with
/// its `token`.
pub fn env(base: &str, token: &str) -> BTreeMap<String, String> {
    let traces = format!("{base}/v1/traces");
    BTreeMap::from([
        ("OTEL_EXPORTER_OTLP_ENDPOINT".into(), base.to_string()),
        ("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT".into(), traces),
        ("OTEL_EXPORTER_OTLP_PROTOCOL".into(), "http/protobuf".into()),
        (
            "OTEL_EXPORTER_OTLP_HEADERS".into(),
            format!("Authorization=Bearer%20{token}"),
        ),
    ])
}

/// What one branch has sent so far: spans, and requests in the current second.
struct Quota {
    spans: usize,
    second: Instant,
    requests: u32,
}

fn quotas() -> &'static Mutex<HashMap<String, Quota>> {
    static Q: OnceLock<Mutex<HashMap<String, Quota>>> = OnceLock::new();
    Q.get_or_init(Default::default)
}

/// Whether `branch` may send another request now: 429 past [`PER_SECOND`]. Forgets branches no
/// longer live once it keeps many.
fn admit(state: &AppState, branch: &str) -> Result<(), Box<Response>> {
    let mut q = quotas()
        .lock()
        .expect("the receiver's quotas are never poisoned");
    if q.len() > 4096 {
        q.retain(|b, _| state.live.contains(b));
    }
    let e = q.entry(branch.to_string()).or_insert(Quota {
        spans: 0,
        second: Instant::now(),
        requests: 0,
    });
    if e.second.elapsed().as_secs() >= 1 {
        (e.second, e.requests) = (Instant::now(), 0);
    }
    e.requests += 1;
    match e.requests > PER_SECOND {
        true => Err(refused(
            StatusCode::TOO_MANY_REQUESTS,
            "too many OTLP requests: at most 20 a second",
        )),
        false => Ok(()),
    }
}

/// The spans of `got` the branch may still keep (the rest are dropped): how many were dropped.
fn take(branch: &str, got: &mut Vec<Imported>) -> usize {
    let mut q = quotas()
        .lock()
        .expect("the receiver's quotas are never poisoned");
    let used = q.get(branch).map_or(0, |e| e.spans);
    let room = BRANCH_SPANS.saturating_sub(used).min(MAX_SPANS);
    let dropped = got.len().saturating_sub(room);
    got.truncate(room);
    if let Some(e) = q.get_mut(branch) {
        e.spans += got.len();
    }
    dropped
}

fn refused(status: StatusCode, why: &str) -> Box<Response> {
    let body = axum::Json(serde_json::json!({ "error": why }));
    Box::new((status, body).into_response())
}

/// The body's format: protobuf (also when unnamed) or JSON; 415 for anything else, compressed
/// bodies among them.
fn json_body(headers: &HeaderMap) -> Result<bool, Box<Response>> {
    let h = |n| {
        headers
            .get(n)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    };
    if !matches!(h(header::CONTENT_ENCODING), "" | "identity") {
        return Err(refused(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "send OTLP uncompressed",
        ));
    }
    match h(header::CONTENT_TYPE)
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
    {
        "" | "application/x-protobuf" | "application/protobuf" => Ok(false),
        "application/json" => Ok(true),
        _ => Err(refused(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "OTLP/HTTP takes application/x-protobuf or application/json",
        )),
    }
}

async fn read(body: Body) -> Result<Bytes, Box<Response>> {
    axum::body::to_bytes(body, MAX_BODY)
        .await
        .map_err(|_| refused(StatusCode::PAYLOAD_TOO_LARGE, "OTLP body over 4 MiB"))
}

/// The answer OTLP/HTTP expects: an `ExportTraceServiceResponse`, saying how many were dropped.
fn answer(json: bool, dropped: usize) -> Response {
    let partial = (dropped > 0).then(|| proto::PartialSuccess {
        rejected: dropped as i64,
        error_message: "the run's span limits were reached".into(),
    });
    if json {
        let body = match &partial {
            Some(p) => {
                serde_json::json!({"partialSuccess": {"rejectedSpans": p.rejected.to_string(), "errorMessage": p.error_message}})
            }
            None => serde_json::json!({}),
        };
        return axum::Json(body).into_response();
    }
    let r = proto::ExportTraceServiceResponse {
        partial_success: partial,
    };
    (
        [(header::CONTENT_TYPE, "application/x-protobuf")],
        r.encode_to_vec(),
    )
        .into_response()
}

/// The live branch `branch`'s agent `agent`, when the request carries its token, and whether the
/// body is JSON.
fn checked(
    state: &AppState,
    (branch, agent): (&str, &str),
    headers: &HeaderMap,
) -> Result<(Arc<crate::exec::BranchCtx>, bool), Box<Response>> {
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    let (ctx, _) = crate::gateway::live_agent(state, branch, agent, auth)
        .map_err(|(status, why)| refused(status, why))?;
    Ok((ctx, json_body(headers)?))
}

/// The spans of the request, decoded and bounded: what is kept, and how many were dropped.
async fn spans_of(
    state: &AppState,
    branch: &str,
    json: bool,
    body: Body,
) -> Result<(Vec<Imported>, usize), Box<Response>> {
    admit(state, branch)?;
    let bytes = read(body).await?;
    let decoded = match json {
        true => inbound::json(&bytes),
        false => inbound::protobuf(&bytes),
    };
    let mut spans = decoded.map_err(|why| refused(StatusCode::BAD_REQUEST, &why))?;
    let dropped = take(branch, &mut spans);
    Ok((spans, dropped))
}

async fn traces(
    State(state): State<Arc<AppState>>,
    Path((branch, agent)): Path<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let received = async {
        let (ctx, json) = checked(&state, (&branch, &agent), &headers)?;
        let (spans, dropped) = spans_of(&state, &branch, json, body).await?;
        if !spans.is_empty() {
            let payload = serde_json::to_value(SpansPayload { spans }).unwrap_or_default();
            if let Err(e) = ctx.append(Some(&agent), EventKind::Spans, payload).await {
                eprintln!("otlp: recording spans on {branch}: {e:#}");
                let why = "the spans could not be recorded";
                return Err(refused(StatusCode::SERVICE_UNAVAILABLE, why));
            }
        }
        Ok(answer(json, dropped))
    };
    received.await.unwrap_or_else(|r| *r)
}

/// Metrics and logs: checked like traces, then dropped.
async fn dropped(
    State(state): State<Arc<AppState>>,
    Path((branch, agent)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    match checked(&state, (&branch, &agent), &headers) {
        Ok((_, json)) => answer(json, 0),
        Err(r) => *r,
    }
}
