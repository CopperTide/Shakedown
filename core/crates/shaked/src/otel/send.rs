//! Sending to a target: OTLP/HTTP protobuf POSTs with its headers, through shaked's guarded client
//! (a shaked that serves others reaches only public HTTPS addresses). A failure says what the
//! collector answered, never a header's value.

use std::time::Duration;

use chrono::Utc;

use super::{encode, span, tree};
use crate::store::{OpenTarget, RunRecord};

/// How long one request may take.
pub const TIMEOUT: Duration = Duration::from_secs(20);

/// `text` with every header value of `t` blanked.
fn redacted(t: &OpenTarget, text: &str) -> String {
    let blank = |acc: String, v: &String| match v.len() >= 4 {
        true => acc.replace(v.as_str(), "••••"),
        false => acc,
    };
    t.headers.values().fold(text.to_string(), blank)
}

/// POSTs `body` (protobuf) to `url` with `t`'s headers.
pub async fn post(
    http: &reqwest::Client,
    t: &OpenTarget,
    url: &str,
    body: Vec<u8>,
) -> Result<(), String> {
    let mut req = http
        .post(url)
        .timeout(TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/x-protobuf")
        .body(body);
    for (k, v) in &t.headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let host = crate::providers::reach::host_of(url, "the collector");
    let r = match req.send().await {
        Ok(r) => r,
        Err(e) => return Err(redacted(t, &format!("{host}: {}", e.without_url()))),
    };
    let status = r.status();
    if status.is_success() {
        return Ok(());
    }
    let text = r.text().await.unwrap_or_default();
    let tail: String = text.chars().take(200).collect();
    Err(redacted(
        t,
        &format!("{host} answered HTTP {}: {tail}", status.as_u16()),
    ))
}

/// Sends `rec`'s spans to `t`: how many.
pub async fn export(
    http: &reqwest::Client,
    t: &OpenTarget,
    rec: &RunRecord,
) -> Result<i32, String> {
    let spans = tree::spans(rec, t.row.flags.include_content);
    for body in encode::requests(&spans, t.row.project.as_deref()) {
        post(http, t, &t.row.endpoint, body).await?;
    }
    Ok(i32::try_from(spans.len()).unwrap_or(i32::MAX))
}

/// One span that says the target works (Test): its own fresh trace.
pub async fn test(http: &reqwest::Client, t: &OpenTarget) -> Result<(), String> {
    let now = Utc::now();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let attrs = span::AttrsBuilder::default()
        .str("shakedown.test", "connection test from Shakedown")
        .str("openinference.span.kind", "CHAIN")
        .build();
    let s = span::SpanOut {
        trace_id: span::ids::trace(&id),
        span_id: span::ids::span(&id, "test"),
        parent: None,
        name: "shakedown connection test".into(),
        kind: span::Kind::Internal,
        start: now,
        end: now,
        attrs,
        events: vec![],
        error: None,
    };
    let bodies = encode::requests(&[s], t.row.project.as_deref());
    for body in bodies {
        post(http, t, &t.row.endpoint, body).await?;
    }
    Ok(())
}
