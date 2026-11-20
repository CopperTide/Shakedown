//! The gateway's two faces against a fake store: ingest restamps and forwards, the query face
//! answers in world time.
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::Router;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};
use prost::Message;
use serde_json::{json, Value};
use worldclock_gateway::clock::{wall_now_ns, Clock, ClockFile};
use worldclock_gateway::{ingest_router, query_router, Gateway, Upstreams};

/// A request the fake store got.
#[derive(Debug, Clone)]
struct Got {
    method: Method,
    uri: String,
    headers: HeaderMap,
    body: Bytes,
}

/// What the fake store answers: status, content type, body.
type Answer = (u16, String, Vec<u8>);

#[derive(Clone)]
struct Store {
    got: Arc<Mutex<Vec<Got>>>,
    answer: Arc<Mutex<Answer>>,
}

impl Store {
    fn answer(&self, status: u16, content_type: &str, body: impl Into<Vec<u8>>) {
        *self.answer.lock().unwrap() = (status, content_type.into(), body.into());
    }

    fn last(&self) -> Got {
        self.got
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("the store got a request")
    }
}

async fn serve(router: Router) -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let at = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, router).await.unwrap() });
    format!("http://{at}")
}

/// A fake store answering every request with its `answer`, and recording it.
async fn store() -> (Store, String) {
    let s = Store {
        got: Arc::default(),
        answer: Arc::new(Mutex::new((200, "application/json".into(), b"{}".to_vec()))),
    };
    let rec = s.clone();
    let any = move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
        let rec = rec.clone();
        async move {
            rec.got.lock().unwrap().push(Got {
                method,
                uri: uri.to_string(),
                headers,
                body,
            });
            let (status, ct, body) = rec.answer.lock().unwrap().clone();
            (
                StatusCode::from_u16(status).unwrap(),
                [("content-type", ct)],
                body,
            )
        }
    };
    let base = serve(Router::new().fallback(any)).await;
    (s, base)
}

/// The gateway's faces (ingest, query) on a clock `offset_ms` from the wall, in front of the store.
async fn gateway(offset_ms: i64) -> (Store, String, String) {
    let (s, base) = store().await;
    let now_ms = (wall_now_ns() / 1_000_000) as i64;
    let clock = Clock::of(ClockFile {
        epoch_ms: 0,
        clock_ms: now_ms + offset_ms,
        speed: 1.0,
        wall_ms: now_ms,
    });
    let up = Upstreams {
        traces: format!("{base}/traces"),
        logs: format!("{base}/logs"),
        metrics: format!("{base}/metrics"),
        prometheus: format!("{base}/prom"),
        loki: format!("{base}/loki"),
        tempo: format!("{base}/tempo"),
    };
    let g = Arc::new(Gateway::new(Arc::new(clock), up));
    let ingest = serve(ingest_router(g.clone())).await;
    let query = serve(query_router(g)).await;
    (s, ingest, query)
}

fn one_span(start: u64) -> ExportTraceServiceRequest {
    let span = Span {
        start_time_unix_nano: start,
        end_time_unix_nano: start + 1_000,
        ..Span::default()
    };
    let scope = ScopeSpans {
        spans: vec![span],
        ..ScopeSpans::default()
    };
    let rs = ResourceSpans {
        scope_spans: vec![scope],
        ..ResourceSpans::default()
    };
    ExportTraceServiceRequest {
        resource_spans: vec![rs],
    }
}

fn start_of(req: &ExportTraceServiceRequest) -> u64 {
    req.resource_spans[0].scope_spans[0].spans[0].start_time_unix_nano
}

const HOUR_MS: i64 = 3_600_000;

#[tokio::test]
async fn ingest_restamps_protobuf_and_passes_the_stores_answer_back() {
    let (s, ingest, _) = gateway(-HOUR_MS).await;
    s.answer(503, "text/plain", "busy");
    let wall = wall_now_ns();
    let body = one_span(wall).encode_to_vec();
    let client = reqwest::Client::new();
    let sent = client
        .post(format!("{ingest}/v1/traces"))
        .header("content-type", "application/x-protobuf")
        .body(body);
    let answer = sent.send().await.unwrap();
    assert_eq!(answer.status().as_u16(), 503);
    assert_eq!(answer.text().await.unwrap(), "busy");
    let got = s.last();
    assert_eq!((got.method, got.uri.as_str()), (Method::POST, "/traces"));
    assert_eq!(got.headers["content-type"], "application/x-protobuf");
    let fwd = ExportTraceServiceRequest::decode(got.body).unwrap();
    let shifted = start_of(&fwd) as i128 - wall as i128;
    assert!(
        (shifted + i128::from(HOUR_MS) * 1_000_000).abs() < 1_100_000_000,
        "{shifted}"
    );
}

#[tokio::test]
async fn ingest_takes_otlp_json_and_forwards_json() {
    let (s, ingest, _) = gateway(-HOUR_MS).await;
    let wall = wall_now_ns();
    let body = json!({"resourceLogs": [{"scopeLogs": [{"logRecords": [
        {"timeUnixNano": wall.to_string(), "body": {"stringValue": "hi"}}]}]}]});
    let client = reqwest::Client::new();
    let sent = client
        .post(format!("{ingest}/v1/logs"))
        .header("content-type", "application/json")
        .body(body.to_string());
    assert_eq!(sent.send().await.unwrap().status().as_u16(), 200);
    let got = s.last();
    assert_eq!(got.uri, "/logs");
    let fwd: Value = serde_json::from_slice(&got.body).unwrap();
    let record = &fwd["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0];
    let t: i128 = record["timeUnixNano"].as_str().unwrap().parse().unwrap();
    assert!(
        (t - wall as i128 + i128::from(HOUR_MS) * 1_000_000).abs() < 1_100_000_000,
        "{record}"
    );
    assert_eq!(record["body"]["stringValue"], "hi");
}

#[tokio::test]
async fn compressed_ingest_is_refused() {
    let (s, ingest, _) = gateway(0).await;
    let client = reqwest::Client::new();
    let sent = client
        .post(format!("{ingest}/v1/metrics"))
        .header("content-encoding", "gzip")
        .body("x");
    assert_eq!(sent.send().await.unwrap().status().as_u16(), 415);
    assert!(s.got.lock().unwrap().is_empty());
}

/// The value of `key` in a query string or form.
fn param(query: &str, key: &str) -> Option<String> {
    let pairs = form_urlencoded::parse(query.as_bytes());
    pairs
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

fn query_of(got: &Got) -> String {
    got.uri
        .split_once('?')
        .map(|(_, q)| q.to_string())
        .unwrap_or_default()
}

#[tokio::test]
async fn instant_queries_default_to_world_now() {
    let (s, _, query) = gateway(-HOUR_MS).await;
    let answer = reqwest::get(format!("{query}/q/prometheus/api/v1/query?query=up"))
        .await
        .unwrap();
    assert_eq!(answer.status().as_u16(), 200);
    let got = s.last();
    assert!(got.uri.starts_with("/prom/api/v1/query?"), "{}", got.uri);
    let time: f64 = param(&query_of(&got), "time").unwrap().parse().unwrap();
    let world_now_s = (wall_now_ns() / 1_000_000) as f64 / 1000.0 - 3600.0;
    assert!((time - world_now_s).abs() < 2.0, "{time} vs {world_now_s}");
    assert_eq!(param(&query_of(&got), "query").as_deref(), Some("up"));
}

#[tokio::test]
async fn a_loki_range_without_end_ends_at_world_now_and_spans_an_hour() {
    let (s, _, query) = gateway(-HOUR_MS).await;
    reqwest::get(format!(
        "{query}/q/loki/loki/api/v1/query_range?query=%7Bx%3D%22y%22%7D"
    ))
    .await
    .unwrap();
    let q = query_of(&s.last());
    let end: i128 = param(&q, "end").unwrap().parse().unwrap();
    let start: i128 = param(&q, "start").unwrap().parse().unwrap();
    let world_now = wall_now_ns() as i128 - i128::from(HOUR_MS) * 1_000_000;
    assert!((end - world_now).abs() < 2_000_000_000, "{end}");
    assert_eq!(end - start, i128::from(HOUR_MS) * 1_000_000);
    assert_eq!(param(&q, "query").as_deref(), Some("{x=\"y\"}"));
}

#[tokio::test]
async fn grafana_queries_are_shifted_both_ways() {
    let (s, _, query) = gateway(-HOUR_MS).await;
    let matrix = json!({"status": "success", "data": {"resultType": "matrix",
        "result": [{"metric": {"a": "b"}, "values": [[6400, "1"], [6415.5, "2"]]}]}});
    s.answer(200, "application/json", matrix.to_string());
    let client = reqwest::Client::new();
    let url =
        format!("{query}/q/prometheus/api/v1/query_range?query=up&start=10000&end=13600&step=15");
    let answer = client
        .get(url)
        .header("X-Shakedown-Shift", "wall")
        .send()
        .await
        .unwrap();
    let q = query_of(&s.last());
    assert_eq!(
        (param(&q, "start").as_deref(), param(&q, "end").as_deref()),
        (Some("6400"), Some("10000"))
    );
    assert_eq!(param(&q, "step").as_deref(), Some("15"));
    assert!(!s.last().headers.contains_key("x-shakedown-shift"));
    let back: Value = answer.json().await.unwrap();
    assert_eq!(
        back["data"]["result"][0]["values"],
        json!([[10000, "1"], [10015.5, "2"]])
    );
    assert_eq!(back["data"]["result"][0]["metric"], json!({"a": "b"}));
}

#[tokio::test]
async fn a_grafana_form_query_is_shifted_too() {
    let (s, _, query) = gateway(-HOUR_MS).await;
    let vector = json!({"status": "success", "data": {"resultType": "vector",
        "result": [{"metric": {}, "value": [6400, "3"]}]}});
    s.answer(200, "application/json", vector.to_string());
    let client = reqwest::Client::new();
    let sent = client
        .post(format!("{query}/q/prometheus/api/v1/query"))
        .header("X-Shakedown-Shift", "wall")
        .header("content-type", "application/x-www-form-urlencoded")
        .body("query=up&time=10000");
    let back: Value = sent.send().await.unwrap().json().await.unwrap();
    let got = s.last();
    let form = String::from_utf8(got.body.to_vec()).unwrap();
    assert_eq!(param(&form, "time").as_deref(), Some("6400"), "{form}");
    assert_eq!(back["data"]["result"][0]["value"], json!([10000, "3"]));
}

/// Grafana's shifted GET of `url` on the query face; the JSON it gets back.
async fn grafana_get(url: String) -> Value {
    let sent = reqwest::Client::new()
        .get(url)
        .header("X-Shakedown-Shift", "wall");
    sent.send().await.unwrap().json().await.unwrap()
}

#[tokio::test]
async fn loki_streams_are_shifted_back_in_nanoseconds() {
    let (s, _, query) = gateway(-HOUR_MS).await;
    let streams = json!({"status": "success", "data": {"resultType": "streams",
        "result": [{"stream": {}, "values": [["1000000000000", "line"]]}]}});
    s.answer(200, "application/json", streams.to_string());
    let url = "/q/loki/loki/api/v1/query_range?query=x&start=4600000000000&end=4700000000000";
    let back = grafana_get(format!("{query}{url}")).await;
    let start = param(&query_of(&s.last()), "start");
    assert_eq!(start.as_deref(), Some("1000000000000"));
    let line = &back["data"]["result"][0]["values"][0];
    assert_eq!(line, &json!(["4600000000000", "line"]));
}

#[tokio::test]
async fn tempo_searches_are_shifted_in_seconds_and_back_in_nanoseconds() {
    let (s, _, query) = gateway(-HOUR_MS).await;
    let search = json!({"traces": [{"traceID": "ab", "startTimeUnixNano": "1000000000000"}]});
    s.answer(200, "application/json", search.to_string());
    let back = grafana_get(format!("{query}/q/tempo/api/search?start=4600&end=4700")).await;
    let start = param(&query_of(&s.last()), "start");
    assert_eq!(start.as_deref(), Some("1000"));
    assert_eq!(back["traces"][0]["startTimeUnixNano"], "4600000000000");
}

#[tokio::test]
async fn queries_are_routed_by_host() {
    let (s, _, query) = gateway(0).await;
    let client = reqwest::Client::new();
    let get = |host: &str, fwd: Option<&str>| {
        let mut r = client
            .get(format!("{query}/api/v1/labels"))
            .header("host", host);
        if let Some(f) = fwd {
            r = r.header("x-forwarded-host", f);
        }
        r.send()
    };
    get("prometheus.shop.test", None).await.unwrap();
    assert_eq!(s.last().uri, "/prom/api/v1/labels");
    get("127.0.0.1:9", Some("loki.shop.test")).await.unwrap();
    assert!(
        s.last().uri.starts_with("/loki/api/v1/labels"),
        "{}",
        s.last().uri
    );
    get("tempo.shop.test:443", None).await.unwrap();
    assert!(
        s.last().uri.starts_with("/tempo/api/v1/labels"),
        "{}",
        s.last().uri
    );
    let n = s.got.lock().unwrap().len();
    let other = get("grafana.shop.test", None).await.unwrap();
    assert_eq!(other.status().as_u16(), 404);
    assert_eq!(s.got.lock().unwrap().len(), n);
}
