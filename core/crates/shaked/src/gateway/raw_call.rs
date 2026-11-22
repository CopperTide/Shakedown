//! One passed-through call, once admitted: sent to the provider on the streaming client (a
//! connect timeout and an idle timeout per read, no limit on the whole), answered as it comes,
//! and recorded once when it ends as a raw `model_call` event. A stream the provider cut is
//! recorded with what came and `interrupted`, the reason: it broke off, or ended without its final
//! event, or said `error` on the way.

use std::{sync::Arc, time::Instant};

use axum::{
    body::{Body, Bytes},
    http::{header::CONTENT_TYPE, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{
    answer, error_response, meter_call,
    passthrough::api_headers,
    redact_message, sse,
    target::{Outcome, Prepared, Route},
    with_cost, Api,
};
use crate::{app::AppState, model::*, overhead::ms_since};

/// Marks a passed-through call's `model_call` event `v` with its API (none for chat completions,
/// the canonical format), `stream` when streamed, and `interrupted` when the stream was cut.
pub(super) fn marked(v: &mut Value, api: Api, interrupted: Option<&str>) {
    if api != Api::Chat {
        v["api"] = json!(api.path());
    }
    if v["request"]["stream"] == true {
        v["stream"] = json!(true);
    }
    if let Some(why) = interrupted {
        v["interrupted"] = json!(why);
    }
}

/// One call on its route, prepared for its provider.
pub(super) struct Call {
    pub state: Arc<AppState>,
    pub route: Route,
    pub prepared: Prepared,
}

/// What the provider answered: its status and answer, and why a stream stopped short.
struct Answered {
    status: u16,
    response: Value,
    interrupted: Option<String>,
}

type Chunks = mpsc::Sender<Result<Bytes, std::io::Error>>;

impl Call {
    /// Sends the request on and answers with what came back.
    pub(super) async fn send(self, headers: &HeaderMap) -> Response {
        let (http, path) = (&self.state.stream_http, self.route.api.path());
        let req = self.prepared.provider.raw_post(http, path);
        let req = req
            .headers(api_headers(headers))
            .json(&self.prepared.request);
        let sent_at = Instant::now();
        match req.send().await {
            Ok(r) if streamed(&self.prepared.request, &r) => self.stream(r, sent_at),
            Ok(r) => self.whole(r, sent_at).await,
            Err(e) => {
                let why = format!("upstream request failed: {}", e.without_url());
                let response = json!({ "error": why });
                let failed = Answered {
                    status: 502,
                    response,
                    interrupted: None,
                };
                self.finish(failed, sent_at).await;
                error_response(StatusCode::BAD_GATEWAY, why)
            }
        }
    }

    /// The answer read whole: recorded, then passed on (a rejected key's message left out).
    async fn whole(self, r: reqwest::Response, sent_at: Instant) -> Response {
        let status = r.status().as_u16();
        let retry_after = (!r.status().is_success()).then(|| retry_after(&r));
        let text = match r.text().await {
            Ok(t) => t,
            Err(e) => format!("upstream response unreadable: {}", e.without_url()),
        };
        let mut body = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }));
        if matches!(status, 401 | 403) {
            redact_message(&mut body);
        }
        let response = body.clone();
        let answered = Answered {
            status,
            response,
            interrupted: None,
        };
        let cost = self.finish(answered, sent_at).await;
        with_cost(answer(status, body, retry_after.flatten()), cost)
    }

    /// The answer streamed on chunk by chunk as it comes; recorded once it ends.
    fn stream(self, r: reqwest::Response, sent_at: Instant) -> Response {
        let kind = r.headers().get(CONTENT_TYPE).cloned();
        let kind = kind.unwrap_or_else(|| HeaderValue::from_static("text/event-stream"));
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(self.relay(r, tx, sent_at));
        let chunks = futures_util::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|chunk| (chunk, rx))
        });
        let body = Body::from_stream(chunks);
        (StatusCode::OK, [(CONTENT_TYPE, kind)], body).into_response()
    }

    /// Passes every chunk of `r` to `tx` (on to the end even when the caller left: the call is
    /// recorded whole), then records the answer the stream gave: cut when it broke off, or ended
    /// without its final event.
    async fn relay(self, r: reqwest::Response, tx: Chunks, sent_at: Instant) {
        let (kept, broke) = pass_on(r, tx).await;
        let text = String::from_utf8_lossy(&kept);
        let interrupted = broke.or_else(|| sse::cut_short(self.route.api, &text));
        let answered = Answered {
            status: 200,
            response: sse::assembled(self.route.api, &text),
            interrupted,
        };
        self.finish(answered, sent_at).await;
    }

    /// Records the call's answer, bills and meters it: what it was charged.
    async fn finish(&self, a: Answered, sent_at: Instant) -> Option<f64> {
        let outcome = Outcome::of(a.status, a.interrupted.as_deref());
        let interrupted = a.interrupted.clone();
        let payload = self.payload(outcome != Outcome::Failed, a);
        let Prepared {
            target,
            provider,
            since,
            ..
        } = &self.prepared;
        let (charge, mut v) = target.bill(&self.route.agent, (provider, &payload), outcome);
        marked(&mut v, self.route.api, interrupted.as_deref());
        target.overhead(*since, ms_since(sent_at));
        self.record(v, &payload.usage, charge.usd).await;
        target.settle(&self.state).await;
        charge.usd
    }

    /// The `model_call` payload of the answer `a` (`ok`: a success, whose usage counts).
    fn payload(&self, ok: bool, a: Answered) -> ModelCallPayload {
        let usage = match ok {
            true => sse::usage(self.route.api, &a.response),
            false => json!({}),
        };
        let p = &self.prepared.provider;
        let cost_usd = ok.then(|| p.cost(&usage)).flatten();
        ModelCallPayload {
            request: self.prepared.request.clone(),
            response: a.response,
            usage,
            cost_usd,
            cost_source: cost_usd.and(p.cost_source()).map(str::to_string),
            provider: Some(p.id.clone()),
            provider_response: None,
        }
    }

    /// Appends the call's `model_call` event `v` and meters the call.
    async fn record(&self, v: Value, usage: &Value, cost: Option<f64>) {
        let agent = &self.route.agent;
        let target = &self.prepared.target;
        let appended = target.append(&self.state, agent, EventKind::ModelCall, v);
        match appended.await {
            Ok(ev) => {
                let paid = (ev.seq, self.prepared.paid_by);
                meter_call(&self.state, target, cost, usage, paid).await;
            }
            Err(e) => {
                let branch = &self.route.branch;
                eprintln!("gateway: recording a model call on {branch}: {e:#}");
            }
        }
    }
}

/// Passes every chunk of `r` to `tx`, to the end even when the caller left: what came, and why
/// it stopped short when the provider's stream failed (an idle read timed out, the connection
/// broke).
async fn pass_on(mut r: reqwest::Response, tx: Chunks) -> (Vec<u8>, Option<String>) {
    let mut kept = Vec::new();
    loop {
        match r.chunk().await {
            Ok(Some(chunk)) => {
                kept.extend_from_slice(&chunk);
                let _ = tx.send(Ok(chunk)).await;
            }
            Ok(None) => return (kept, None),
            Err(e) => {
                let why = format!("the provider's stream stopped: {}", e.without_url());
                let _ = tx.send(Err(std::io::Error::other(why.clone()))).await;
                return (kept, Some(why));
            }
        }
    }
}

/// True when the call asked for a stream and the provider is streaming it.
fn streamed(request: &Value, r: &reqwest::Response) -> bool {
    let kind = r.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok());
    let sse = kind.is_some_and(|k| k.starts_with("text/event-stream"));
    request["stream"] == true && r.status().is_success() && sse
}

/// The whole seconds of the answer's `retry-after`.
fn retry_after(r: &reqwest::Response) -> Option<u64> {
    let v = r.headers().get(reqwest::header::RETRY_AFTER)?;
    v.to_str().ok()?.trim().parse().ok()
}
