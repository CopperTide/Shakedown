//! The REST helper against a one-request server on loopback: answers, refusals, a gateway that
//! says the service is down, and a service that can't be reached.
#![cfg(feature = "http")]
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use arena_part::helpers::http::{is_gateway_error, Answer, Dialect, Reply, Rest};
use arena_part::proto::ToolOutput;
use serde_json::{json, Value};

fn said(a: &Answer) -> String {
    a.body["error"].as_str().map_or_else(
        || format!("Demo answered HTTP {}", a.status),
        str::to_string,
    )
}

fn inner(body: Value) -> Value {
    body.get("data").cloned().unwrap_or(body)
}

static DEMO: Dialect = Dialect {
    service: "Demo",
    gateway_is_down: true,
    refusal: said,
    data: inner,
};

static LENIENT: Dialect = Dialect {
    gateway_is_down: false,
    ..DEMO
};

/// Serves one request with `status` and `body` on a fresh port; the port and the request it got.
fn serve_once(status: u16, body: &'static str) -> (u16, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let served = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut got = vec![0u8; 8192];
        let n = s.read(&mut got).unwrap();
        let answer = format!(
            "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(answer.as_bytes()).unwrap();
        String::from_utf8_lossy(&got[..n]).to_string()
    });
    (port, served)
}

fn rest(dialect: &'static Dialect, port: u16) -> Rest {
    Rest::new(dialect, port, Duration::from_secs(5)).unwrap()
}

#[test]
fn only_502_to_504_are_gateway_errors() {
    let down: Vec<u16> = (400..600).filter(|s| is_gateway_error(*s)).collect();
    assert_eq!(down, [502, 503, 504]);
}

#[test]
fn segments_are_percent_encoded_and_paths_are_kept_as_given() {
    let r = rest(&DEMO, 8080);
    let url = r.url(&["api", "resource", "Sales Order", "a/b"]);
    assert_eq!(
        url.as_str(),
        "http://127.0.0.1:8080/api/resource/Sales%20Order/a%2Fb"
    );
    assert_eq!(
        r.at("rest/companies/x"),
        "http://127.0.0.1:8080/rest/companies/x"
    );
}

#[tokio::test]
async fn a_2xx_answer_is_the_dialects_data_and_auth_is_sent() {
    let (port, served) = serve_once(200, r#"{"data": {"id": 7}}"#);
    let r = rest(&DEMO, port).with_auth("token k:s".into());
    let reply = r.reply(r.request(reqwest::Method::GET, r.at("x"))).await;
    assert_eq!(reply.unwrap(), Reply::Ok(json!({"id": 7})));
    let request = served.join().unwrap().to_lowercase();
    assert!(request.contains("authorization: token k:s"), "{request}");
}

#[tokio::test]
async fn a_refusal_carries_the_status_and_the_dialects_message() {
    let (port, _served) = serve_once(417, r#"{"error": "no such item"}"#);
    let r = rest(&DEMO, port);
    let reply = r.reply(r.request(reqwest::Method::GET, r.at("x"))).await;
    let want = Reply::Refused {
        status: 417,
        message: "no such item".into(),
    };
    assert_eq!(reply.unwrap(), want);
    let (port, _served) = serve_once(400, "not json");
    let r = rest(&DEMO, port);
    let reply = r.reply(r.request(reqwest::Method::GET, r.at("x"))).await;
    let message = match reply.unwrap() {
        Reply::Refused { message, .. } => message,
        other => panic!("{other:?}"),
    };
    assert_eq!(message, "Demo answered HTTP 400");
}

#[tokio::test]
async fn a_gateway_error_is_the_service_down_unless_the_dialect_says_otherwise() {
    let (port, _served) = serve_once(503, "");
    let r = rest(&DEMO, port);
    let err = r.reply(r.request(reqwest::Method::GET, r.at("x"))).await;
    assert_eq!(
        err.unwrap_err().to_string(),
        "Demo is not answering (HTTP 503)"
    );
    let (port, _served) = serve_once(503, r#"{"error": "busy"}"#);
    let r = rest(&LENIENT, port);
    let reply = r.reply(r.request(reqwest::Method::GET, r.at("x"))).await;
    let want = Reply::Refused {
        status: 503,
        message: "busy".into(),
    };
    assert_eq!(reply.unwrap(), want);
}

#[tokio::test]
async fn a_service_that_cant_be_reached_fails_the_call() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let r = rest(&DEMO, port);
    let err = r.reply(r.request(reqwest::Method::GET, r.at("x"))).await;
    assert_eq!(err.unwrap_err().to_string(), "Demo can't be reached");
}

#[test]
fn a_reply_is_a_tool_output_and_accepted_is_its_value() {
    let ok = ToolOutput::from(Reply::Ok(json!({"a": 1})));
    assert_eq!((ok.result, ok.error), (Some(json!({"a": 1})), None));
    let refused = Reply::Refused {
        status: 404,
        message: "gone".into(),
    };
    assert_eq!(refused.clone().accepted(), Err("gone".to_string()));
    let out = ToolOutput::from(refused);
    assert_eq!((out.result, out.error), (None, Some("gone".to_string())));
    assert_eq!(Reply::Ok(json!(1)).accepted(), Ok(json!(1)));
}
