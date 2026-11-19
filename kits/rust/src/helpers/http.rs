//! A JSON REST client for an integration's service on loopback (feature `http`): one timeout, an
//! optional `Authorization` header, and one rule for what an answer means. A 2xx answer is the
//! service's data; any other answer is a refusal (a tool error, not a failure of the integration);
//! a service that can't be reached, or (when its [`Dialect`] says so) a gateway error in front of
//! it, fails the call.
use std::time::Duration;

use anyhow::{bail, Context, Result};
pub use reqwest;
use reqwest::header::AUTHORIZATION;
use reqwest::{Client, IntoUrl, Method, RequestBuilder, Url};
use serde_json::Value;

use crate::proto::ToolOutput;

/// How a service answers: its name in errors, where its message and data are.
#[derive(Debug)]
pub struct Dialect {
    /// The service's name in errors ("ERPNext can't be reached").
    pub service: &'static str,
    /// Whether a gateway error (502–504) means the service is down rather than a refusal.
    pub gateway_is_down: bool,
    /// The message of a refusal.
    pub refusal: fn(&Answer) -> String,
    /// The data of a 2xx answer, from its body.
    pub data: fn(Value) -> Value,
}

/// An answer as read: its status, its text, and the text as JSON (null when it isn't).
#[derive(Debug, Clone)]
pub struct Answer {
    pub status: u16,
    pub text: String,
    pub body: Value,
}

/// What the service answered: its data, or why it refused.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Ok(Value),
    /// The HTTP status and the service's message.
    Refused {
        status: u16,
        message: String,
    },
}

impl Reply {
    /// The data, or the refusal's message.
    pub fn accepted(self) -> Result<Value, String> {
        match self {
            Reply::Ok(v) => Ok(v),
            Reply::Refused { message, .. } => Err(message),
        }
    }
}

impl From<Reply> for ToolOutput {
    /// The data as the tool's result, or the refusal as its error.
    fn from(reply: Reply) -> ToolOutput {
        let (result, error) = match reply.accepted() {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e)),
        };
        ToolOutput {
            result,
            error,
            ..ToolOutput::default()
        }
    }
}

/// Whether `status` is a gateway's (502–504): the service behind it is not answering.
pub fn is_gateway_error(status: u16) -> bool {
    matches!(status, 502..=504)
}

#[derive(Debug, Clone)]
pub struct Rest {
    client: Client,
    base: Url,
    auth: Option<String>,
    dialect: &'static Dialect,
}

impl Rest {
    /// The service on 127.0.0.1:`port`, each request allowed `timeout`, without auth.
    pub fn new(dialect: &'static Dialect, port: u16, timeout: Duration) -> Result<Rest> {
        Ok(Rest {
            client: Client::builder().timeout(timeout).build()?,
            base: Url::parse(&format!("http://127.0.0.1:{port}/"))?,
            auth: None,
            dialect,
        })
    }

    /// The same, sending `value` as the `Authorization` header of every request.
    pub fn with_auth(&self, value: String) -> Rest {
        Rest {
            auth: Some(value),
            ..self.clone()
        }
    }

    /// `/<segments..>`, each segment percent-encoded (names may hold spaces and `/`).
    pub fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("an http URL has path segments")
            .extend(segments);
        url
    }

    /// `/<path>`, as given.
    pub fn at(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// A request to `url`, with the auth.
    pub fn request(&self, method: Method, url: impl IntoUrl) -> RequestBuilder {
        let r = self.client.request(method, url);
        match &self.auth {
            Some(a) => r.header(AUTHORIZATION, a),
            None => r,
        }
    }

    /// Sends `r` and reads the answer; an error when the service can't be reached or is down.
    pub async fn answer(&self, r: RequestBuilder) -> Result<Answer> {
        let name = self.dialect.service;
        let resp = r
            .send()
            .await
            .with_context(|| format!("{name} can't be reached"))?;
        let status = resp.status().as_u16();
        if self.dialect.gateway_is_down && is_gateway_error(status) {
            bail!("{name} is not answering (HTTP {status})");
        }
        let text = resp.text().await;
        let text = text.with_context(|| format!("reading {name}'s answer"))?;
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(Answer { status, text, body })
    }

    /// Sends `r`: a 2xx answer's data, any other answer as `Refused`.
    pub async fn reply(&self, r: RequestBuilder) -> Result<Reply> {
        let a = self.answer(r).await?;
        if (200..300).contains(&a.status) {
            return Ok(Reply::Ok((self.dialect.data)(a.body)));
        }
        let message = (self.dialect.refusal)(&a);
        Ok(Reply::Refused {
            status: a.status,
            message,
        })
    }
}
