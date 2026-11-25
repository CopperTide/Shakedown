//! Observability against a local fake OTLP collector (no network beyond 127.0.0.1): targets keep
//! their headers sealed and write-only; an ended run is exported once, from its record, with
//! stable span ids, retried after a failure; Prometheus text at `/metrics`; and the OTLP receiver
//! that takes an external agent's own spans: its token, its caps, its formats.
mod support;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use axum::{body::Bytes, extract::State, http::HeaderMap, routing::post, Router};
use prost::Message;
use serde_json::{json, Value};
use shaked::{model::*, otel, vault::Vault};
use support::{app::bind, records, run, AppFx};

const K: &str = "0001020304050607080910111213141516171819202122232425262728293031";
const SECRET: &str = "hc-secret-value-1234";

/// What the fake collector got: each request's path, its header, and its body.
#[derive(Default)]
struct Got {
    seen: Mutex<Vec<(String, Option<String>, Bytes)>>,
    /// Requests to answer 503 before answering 200.
    fail: AtomicUsize,
}

async fn collect(
    State(g): State<Arc<Got>>,
    uri: axum::http::Uri,
    h: HeaderMap,
    body: Bytes,
) -> axum::http::StatusCode {
    let team = h
        .get("x-honeycomb-team")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    g.seen
        .lock()
        .unwrap()
        .push((uri.path().to_string(), team, body));
    match g
        .fail
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
    {
        Ok(_) => axum::http::StatusCode::SERVICE_UNAVAILABLE,
        Err(_) => axum::http::StatusCode::OK,
    }
}

/// A fake OTLP/HTTP collector: its state and base URL.
async fn collector() -> (Arc<Got>, String) {
    let (listener, base) = bind().await;
    let got = Arc::new(Got::default());
    let app = Router::new()
        .route("/v1/traces", post(collect))
        .route("/v1/metrics", post(collect))
        .with_state(got.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (got, base)
}

async fn app(tag: &str) -> AppFx {
    let fx = AppFx::idle("otel", tag).start().await;
    let _ = fx.state.vault.set(Arc::new(Vault::from_hex(K).unwrap()));
    fx
}

/// Adds a generic OTLP target at `base`: its row.
async fn add_target(fx: &AppFx, base: &str, auto: bool) -> Value {
    let body = json!({"name": "honeycomb", "kind": "otlp", "endpoint": base,
                      "headers": {"x-honeycomb-team": SECRET}, "auto_export": auto,
                      "export_metrics": true});
    let (status, row) = fx.post("/api/observability/targets", body).await;
    assert_eq!(status, 200, "{row}");
    row
}

/// An external run with agent `a1`, its branch and the agent's endpoints.
async fn external_run(fx: &AppFx) -> (String, Value) {
    let team = json!({"id": "own", "name": "Own design", "agents": []});
    let body = json!({"task_id": "27", "team": team, "runner": "external"});
    let (status, v) = fx.post("/api/runs", body).await;
    assert_eq!(status, 200, "{v}");
    let branch = v["branch_id"].as_str().unwrap().to_string();
    let path = format!("/api/branches/{branch}/agents");
    let (status, ends) = fx.post(&path, json!({"id": "a1", "name": "Agent"})).await;
    assert_eq!(status, 200, "{ends}");
    (branch, ends)
}

async fn finish(fx: &AppFx, branch: &str) {
    let (status, v) = fx
        .post(&format!("/api/branches/{branch}/finish"), json!({}))
        .await;
    assert_eq!(status, 200, "{v}");
}

/// The span ids of every trace request the collector got.
fn span_ids(got: &Got) -> Vec<Vec<u8>> {
    let seen = got.seen.lock().unwrap();
    let traces = seen.iter().filter(|(p, ..)| p == "/v1/traces");
    let decoded =
        traces.map(|(_, _, b)| otel::proto::ExportTraceServiceRequest::decode(&b[..]).unwrap());
    let spans = decoded
        .flat_map(|r| r.resource_spans)
        .flat_map(|r| r.scope_spans)
        .flat_map(|s| s.spans);
    spans.map(|s| s.span_id).collect()
}

#[test]
fn a_target_s_headers_are_sealed_and_never_shown() {
    run(async {
        let fx = app("sealed").await;
        let (_got, base) = collector().await;
        let made = add_target(&fx, &base, true).await;
        let (_, list) = fx.get("/api/observability/targets").await;
        for text in [made.to_string(), list.to_string()] {
            assert!(!text.contains(SECRET), "{text}");
        }
        assert_eq!(list[0]["header_names"], json!(["x-honeycomb-team"]));
        assert_eq!(list[0]["endpoint"], format!("{base}/v1/traces"));
        assert_eq!(
            list[0]["include_content"], true,
            "a collector of yours takes content"
        );
        let dup = json!({"name": "honeycomb", "kind": "otlp", "endpoint": base});
        assert_eq!(fx.post("/api/observability/targets", dup).await.0, 409);
        let bad = json!({"name": "x", "kind": "langfuse", "region": "us"});
        assert_eq!(
            fx.post("/api/observability/targets", bad).await.0,
            400,
            "keys needed"
        );
    })
}

#[test]
fn test_sends_one_span_with_the_target_s_headers() {
    run(async {
        let fx = app("test").await;
        let (got, base) = collector().await;
        let id = add_target(&fx, &base, false).await["id"]
            .as_str()
            .unwrap()
            .to_string();
        let (status, row) = fx
            .post(&format!("/api/observability/targets/{id}/test"), json!({}))
            .await;
        assert_eq!((status, &row["checked_ok"]), (200, &json!(true)), "{row}");
        let seen = got.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0].1.as_deref(),
            Some(SECRET),
            "the sealed header went with it"
        );
    })
}

/// The exports of `branch`.
async fn exports_of(fx: &AppFx, branch: &str) -> Value {
    let (status, ex) = fx
        .get(&format!("/api/observability/exports?branch={branch}"))
        .await;
    assert_eq!(status, 200, "{ex}");
    ex
}

async fn tick(fx: &AppFx) -> usize {
    otel::exporter::tick(&fx.state).await.unwrap()
}

#[test]
fn an_ended_run_is_exported_once_and_retried_after_a_failure() {
    run(async {
        let fx = app("export").await;
        let (got, base) = collector().await;
        add_target(&fx, &base, true).await;
        let (branch, _) = external_run(&fx).await;
        assert_eq!(tick(&fx).await, 0, "not while it runs");
        finish(&fx, &branch).await;
        got.fail.store(1, Ordering::SeqCst);
        assert_eq!(tick(&fx).await, 1);
        let ex = exports_of(&fx, &branch).await;
        let row = (&ex[0]["state"], &ex[0]["attempts"]);
        assert_eq!(row, (&json!("pending"), &json!(1)), "{ex}");
        let why = ex[0]["last_error"].as_str().unwrap();
        assert!(why.contains("503") && !why.contains(SECRET), "{ex}");
        assert_eq!(tick(&fx).await, 0, "not due yet: backoff");
    })
}

#[test]
fn a_run_exported_again_sends_the_same_spans() {
    run(async {
        let fx = app("again").await;
        let (got, base) = collector().await;
        add_target(&fx, &base, false).await;
        let (branch, _) = external_run(&fx).await;
        finish(&fx, &branch).await;
        assert_eq!(tick(&fx).await, 0, "this target exports only when asked");
        for _ in 0..2 {
            let path = format!("/api/branches/{branch}/export");
            let (status, v) = fx.post(&path, json!({})).await;
            let state = &v["exports"][0]["state"];
            assert_eq!((status, state), (200, &json!("done")), "{v}");
        }
        let ids = span_ids(&got);
        let half = ids.len() / 2;
        assert!(half > 0 && ids[..half] == ids[half..], "same span ids");
        assert_eq!(exports_of(&fx, &branch).await[0]["state"], "done");
        assert_eq!(tick(&fx).await, 0, "done is done");
    })
}

#[test]
fn backoff_grows_and_gives_up() {
    use shaked::store::{backoff, MAX_ATTEMPTS};
    let waits: Vec<i64> = (1..=MAX_ATTEMPTS)
        .map(|a| backoff(a).num_seconds())
        .collect();
    assert!(waits.windows(2).all(|w| w[0] <= w[1]), "{waits:?}");
    assert_eq!(waits[0], 30);
}

/// `GET /metrics` of `fx`: its content type and text.
async fn scrape(fx: &AppFx) -> (String, String) {
    let r = fx.http.get(format!("{}/metrics", fx.base)).send();
    let r = r.await.unwrap();
    assert_eq!(r.status(), 200);
    let kind = r.headers()["content-type"].to_str().unwrap().to_string();
    (kind, r.text().await.unwrap())
}

/// The names of the metrics the collector got pushed.
fn pushed(got: &Got) -> Vec<String> {
    let seen = got.seen.lock().unwrap();
    let (_, _, body) = seen.iter().find(|(p, ..)| p == "/v1/metrics").unwrap();
    let m = otel::proto::ExportMetricsServiceRequest::decode(&body[..]).unwrap();
    let metrics = &m.resource_metrics[0].scope_metrics[0].metrics;
    metrics.iter().map(|m| m.name.clone()).collect()
}

#[test]
fn metrics_are_prometheus_text_and_pushed_as_otlp() {
    run(async {
        let fx = app("metrics").await;
        let (got, base) = collector().await;
        add_target(&fx, &base, true).await;
        let (branch, _) = external_run(&fx).await;
        finish(&fx, &branch).await;
        let (kind, text) = scrape(&fx).await;
        assert!(kind.starts_with("text/plain; version=0.0.4"), "{kind}");
        assert!(text.contains("# TYPE shakedown_branches gauge"), "{text}");
        let bucket = "shakedown_run_duration_seconds_bucket{world=";
        assert!(text.contains(bucket), "{text}");
        assert!(text.contains("le=\"+Inf\"} 1"), "{text}");
        assert_eq!(otel::exporter::push_metrics(&fx.state).await.unwrap(), 1);
        let names = pushed(&got);
        assert!(names.iter().any(|n| n == "shakedown_branches"), "{names:?}");
    })
}

/// One span as an agent's OTel SDK sends it.
fn sdk_request(n: usize) -> Vec<u8> {
    let span = |i: usize| otel::proto::Span {
        trace_id: vec![7; 16],
        span_id: (i as u64 + 1).to_be_bytes().to_vec(),
        name: format!("step {i}"),
        kind: 1,
        start_time_unix_nano: 1_700_000_000_000_000_000,
        end_time_unix_nano: 1_700_000_000_500_000_000,
        ..Default::default()
    };
    let req = otel::proto::ExportTraceServiceRequest {
        resource_spans: vec![otel::proto::ResourceSpans {
            resource: None,
            scope_spans: vec![otel::proto::ScopeSpans {
                scope: None,
                spans: (0..n).map(span).collect(),
            }],
        }],
    };
    req.encode_to_vec()
}

/// An agent's OTLP sender: its traces URL, with the authorization it sends.
struct Sender<'a> {
    fx: &'a AppFx,
    url: String,
}

impl Sender<'_> {
    async fn send(&self, auth: Option<&str>, body: Vec<u8>) -> reqwest::Response {
        let mut r = self.fx.http.post(&self.url).body(body);
        r = r.header("content-type", "application/x-protobuf");
        if let Some(a) = auth {
            r = r.header("authorization", a);
        }
        r.send().await.unwrap()
    }
}

/// The `spans` events of `branch`.
async fn spans_events(fx: &AppFx, branch: &str) -> Vec<Event> {
    let evs = records(&fx.state.store).events(branch).await.unwrap();
    evs.into_iter()
        .filter(|e| e.kind == EventKind::Spans)
        .collect()
}

#[test]
fn an_agent_s_endpoints_point_its_otel_sdk_at_its_run() {
    run(async {
        let fx = app("receiveenv").await;
        let (_, ends) = external_run(&fx).await;
        let env = &ends["env"];
        let traces = format!("{}/v1/traces", ends["otlp_url"].as_str().unwrap());
        assert_eq!(env["OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"], traces);
        let token = ends["token"].as_str().unwrap();
        let auth = format!("Authorization=Bearer%20{token}");
        assert_eq!(env["OTEL_EXPORTER_OTLP_HEADERS"], auth);
        assert_eq!(env["OTEL_EXPORTER_OTLP_PROTOCOL"], "http/protobuf");
    })
}

#[test]
fn an_agent_s_own_spans_are_received_with_its_token_into_its_record() {
    run(async {
        let fx = app("receive").await;
        let (branch, ends) = external_run(&fx).await;
        let url = format!("{}/v1/traces", ends["otlp_url"].as_str().unwrap());
        let s = Sender { fx: &fx, url };
        assert_eq!(s.send(None, sdk_request(1)).await.status(), 401);
        assert_eq!(
            s.send(Some("Bearer no"), sdk_request(1)).await.status(),
            401
        );
        let bearer = format!("Bearer {}", ends["token"].as_str().unwrap());
        let ok = s.send(Some(&bearer), sdk_request(2)).await;
        assert_eq!(ok.status(), 200);
        let events = spans_events(&fx, &branch).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].agent.as_deref(), Some("a1"));
        assert_eq!(events[0].payload["spans"][1]["name"], "step 1");
        finish(&fx, &branch).await;
        let gone = s.send(Some(&bearer), sdk_request(1)).await;
        assert_eq!(gone.status(), 404, "a finished branch takes no more spans");
    })
}

#[test]
fn the_receiver_bounds_what_one_request_brings() {
    run(async {
        let fx = app("receivecaps").await;
        let (branch, ends) = external_run(&fx).await;
        let url = format!("{}/v1/traces", ends["otlp_url"].as_str().unwrap());
        let s = Sender { fx: &fx, url };
        let bearer = format!("Bearer {}", ends["token"].as_str().unwrap());
        let big = vec![0; otel::receive::MAX_BODY + 1];
        assert_eq!(s.send(Some(&bearer), big).await.status(), 413);
        let r = s.send(Some(&bearer), sdk_request(otel::receive::MAX_SPANS + 3));
        let body = r.await.bytes().await.unwrap();
        let answer = otel::proto::ExportTraceServiceResponse::decode(body).unwrap();
        assert_eq!(
            answer.partial_success.unwrap().rejected,
            3,
            "dropped, and said so"
        );
        let events = spans_events(&fx, &branch).await;
        let kept = events[0].payload["spans"].as_array().unwrap().len();
        assert_eq!(kept, otel::receive::MAX_SPANS);
    })
}

/// One OTLP/JSON span named `plan`.
fn json_request() -> Value {
    json!({"resourceSpans": [{"scopeSpans": [{"spans": [{
        "traceId": "5b8efff798038103d269b633813fc60c", "spanId": "eee19b7ec3c1b174",
        "name": "plan", "startTimeUnixNano": "1700000000000000000",
        "endTimeUnixNano": "1700000001000000000"}]}]}]})
}

#[test]
fn the_receiver_takes_otlp_json_and_refuses_compressed_bodies() {
    run(async {
        let fx = app("receivejson").await;
        let (branch, ends) = external_run(&fx).await;
        let url = format!("{}/v1/traces", ends["otlp_url"].as_str().unwrap());
        let auth = format!("Bearer {}", ends["token"].as_str().unwrap());
        let post = |url: &str| fx.http.post(url).header("authorization", &auth);
        let r = post(&url).json(&json_request()).send().await.unwrap();
        assert_eq!(r.status(), 200);
        let gz = post(&url)
            .header("content-encoding", "gzip")
            .json(&json_request());
        assert_eq!(gz.send().await.unwrap().status(), 415);
        let metrics = post(&url.replace("/traces", "/metrics")).body(vec![]);
        assert_eq!(
            metrics.send().await.unwrap().status(),
            200,
            "taken, dropped"
        );
        let events = spans_events(&fx, &branch).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["spans"][0]["name"], "plan");
    })
}
