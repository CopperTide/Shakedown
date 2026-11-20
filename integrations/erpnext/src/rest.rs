//! Frappe's REST API on the stack's frontend, one site per request (its `Host` header).
use std::time::Duration;

use anyhow::Result;
use reqwest::header::HOST;
use reqwest::{Method, RequestBuilder};
use serde_json::Value;
use shakedown_part::helpers::http::{self, Answer, Dialect};

pub use shakedown_part::helpers::http::Reply;

use crate::frappe::{error_text, ApiKey};

/// How long one request may take (a big submit can be slow).
const TIMEOUT: Duration = Duration::from_secs(300);

fn refusal(a: &Answer) -> String {
    error_text(a.status, &a.body)
}

/// A 2xx answer's `data` (resources) or `message` (methods).
fn data(body: Value) -> Value {
    let data = ["data", "message"]
        .iter()
        .find_map(|k| body.get(*k).cloned());
    data.unwrap_or(body)
}

static FRAPPE: Dialect = Dialect {
    service: "ERPNext",
    gateway_is_down: true,
    refusal,
    data,
};

#[derive(Debug, Clone)]
pub struct Rest(http::Rest);

impl Rest {
    /// The frontend on 127.0.0.1:`port`, without a key.
    pub fn new(port: u16) -> Result<Rest> {
        Ok(Rest(http::Rest::new(&FRAPPE, port, TIMEOUT)?))
    }

    /// The same, sending `key` with every request.
    pub fn with_key(&self, key: &ApiKey) -> Rest {
        Rest(self.0.with_auth(key.header()))
    }

    /// `method /api/<segments..>` on `site`, each segment percent-encoded.
    fn request(&self, method: Method, site: &str, segments: &[&str]) -> RequestBuilder {
        let url = self.0.url(&[&["api"], segments].concat());
        self.0.request(method, url).header(HOST, site)
    }

    /// `GET /api/<segments>?<query>` on `site`.
    pub async fn get(
        &self,
        site: &str,
        segments: &[&str],
        query: &[(&str, String)],
    ) -> Result<Reply> {
        let r = self.request(Method::GET, site, segments).query(query);
        self.0.reply(r).await
    }

    /// `method /api/<segments>` on `site` with a JSON body.
    pub async fn send(
        &self,
        method: Method,
        site: &str,
        segments: &[&str],
        body: &Value,
    ) -> Result<Reply> {
        let r = self.request(method, site, segments).json(body);
        self.0.reply(r).await
    }

    /// Whether the frontend and the backend behind it answer (any site, any answer but a
    /// gateway error).
    pub async fn answers(&self) -> bool {
        let r = self
            .request(Method::GET, "localhost", &["method", "ping"])
            .send()
            .await;
        r.is_ok_and(|r| !http::is_gateway_error(r.status().as_u16()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_resources_data_or_a_methods_message_else_the_body() {
        assert_eq!(data(json!({"data": {"name": "x"}})), json!({"name": "x"}));
        assert_eq!(data(json!({"message": "pong"})), json!("pong"));
        assert_eq!(data(json!({"other": 1})), json!({"other": 1}));
    }
}
