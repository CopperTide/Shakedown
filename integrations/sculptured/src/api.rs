//! CopperTide's HTTP API on one instance's loopback port.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use arena_part::helpers::http::{self, Answer, Dialect};
use reqwest::Method;
use serde_json::Value;

pub use arena_part::helpers::http::Reply;

/// How long one request may take.
const TIMEOUT: Duration = Duration::from_secs(30);

/// CopperTide's message in `{"error": {code, message}}`, else the answer's text.
fn refusal(a: &Answer) -> String {
    a.body["error"]["message"]
        .as_str()
        .map_or_else(|| a.text.trim().to_string(), str::to_string)
}

/// CopperTide answers on loopback itself: a gateway status is its own refusal.
static COPPERTIDE: Dialect = Dialect {
    service: "CopperTide",
    gateway_is_down: false,
    refusal,
    data: std::convert::identity,
};

/// What CopperTide answered, for a request that must not be refused.
pub trait Expect {
    /// The JSON of the answer (`what` names the request).
    fn expect(self, what: &str) -> Result<Value>;
}

impl Expect for Reply {
    fn expect(self, what: &str) -> Result<Value> {
        match self {
            Reply::Ok(v) => Ok(v),
            Reply::Refused { status, message } => {
                bail!("CopperTide refused {what} (HTTP {status}): {message}")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Api(http::Rest);

impl Api {
    /// The server on 127.0.0.1:`port`.
    pub fn new(port: u16) -> Result<Api> {
        Ok(Api(http::Rest::new(&COPPERTIDE, port, TIMEOUT)?))
    }

    /// `/api/<segments..>`, each segment percent-encoded (seat ids hold `@`).
    fn url(&self, segments: &[&str]) -> reqwest::Url {
        self.0.url(&[&["api"], segments].concat())
    }

    /// `GET /api/<segments>?<query>`.
    pub async fn get(&self, segments: &[&str], query: &[(&str, String)]) -> Result<Reply> {
        let r = self.0.request(Method::GET, self.url(segments));
        self.0.reply(r.query(query)).await
    }

    /// `method /api/<segments>` with `body`, an object, plus a fresh `request_id` (CopperTide
    /// answers a repeated id with the first answer, and a restored store holds old ids).
    pub async fn send(&self, method: Method, segments: &[&str], body: Value) -> Result<Reply> {
        let mut body = body;
        body["request_id"] = Value::String(request_id());
        let r = self.0.request(method, self.url(segments));
        self.0.reply(r.json(&body)).await
    }
}

/// An id no request of this program used before.
fn request_id() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = COUNT.fetch_add(1, Ordering::Relaxed);
    format!("arena-{}-{nanos}-{n}", std::process::id())
}
