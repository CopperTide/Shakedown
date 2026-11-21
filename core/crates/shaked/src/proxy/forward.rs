//! Sending a proxied request on to the part's upstream: the world's host name kept, the scheme,
//! host and port it was asked on forwarded, hop-by-hop and proxy headers dropped.

use std::{sync::LazyLock, time::Duration};

use bytes::Bytes;
use parts::proto::{
    Effect, AGENT_HEADER, EFFECT_HEADER, OLD_AGENT_HEADER, OLD_EFFECT_HEADER, OLD_READONLY_HEADER,
    READONLY_HEADER,
};

use crate::{exec::HttpCall, world::HostRoute};

/// The client for upstreams: never through a proxy, never following redirects (the agent's
/// client sees them).
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("the proxy's upstream client")
});

/// Headers that belong to one connection, not to the request: never passed on.
const HOP_BY_HOP: [&str; 9] = [
    "connection",
    "keep-alive",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "expect",
    "host",
    "content-length",
];

/// True for a header that is not passed on either way.
pub fn is_hop(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("proxy-") || HOP_BY_HOP.contains(&name.as_str())
}

/// True for a header the proxy sets itself (`x-forwarded-*`, `forwarded`, `x-shakedown-*` and the
/// old `x-arena-*`): a client's own copy is dropped, so an upstream can trust what it reads there.
fn is_set_by_proxy(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "forwarded"
        || ["x-forwarded-", "x-shakedown-", "x-arena-"]
            .iter()
            .any(|p| name.starts_with(p))
}

/// What the upstream answered, and the effect it said the request had.
pub struct Forwarded {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
    pub effect: Option<Effect>,
}

/// The `Host` the upstream sees: the world's host, with the port when it isn't the scheme's.
fn host_header(route: &HostRoute, call: &HttpCall) -> String {
    match (call.scheme, call.port) {
        ("http", 80) | ("https", 443) => route.host.clone(),
        (_, port) => format!("{}:{port}", route.host),
    }
}

/// Who sent a request, as the upstream is told: the agent (`AGENT_HEADER`) and, for a route that
/// honours read-only, that the agent may only read (`READONLY_HEADER`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sender<'a> {
    pub agent: Option<&'a str>,
    pub readonly: bool,
}

/// Sends `call` from `sender` to `route`'s upstream; the error when the upstream doesn't answer.
pub async fn forward(
    route: &HostRoute,
    call: &HttpCall,
    sender: Sender<'_>,
) -> Result<Forwarded, String> {
    let method = reqwest::Method::from_bytes(call.method.as_bytes()).map_err(|e| e.to_string())?;
    let mut req = CLIENT.request(method, route.target(&call.path_and_query));
    let passed = |(k, _): &&(String, String)| !is_hop(k) && !is_set_by_proxy(k);
    for (k, v) in call.headers.iter().filter(passed) {
        req = req.header(k, v);
    }
    let resp = (own_headers(req, route, call, sender).body(call.body.clone()))
        .send()
        .await
        .map_err(|e| error_chain(&e))?;
    answered(resp).await
}

/// The headers the proxy sets itself: the world's `Host`, `x-forwarded-*` and who sent it.
fn own_headers(
    req: reqwest::RequestBuilder,
    route: &HostRoute,
    call: &HttpCall,
    sender: Sender<'_>,
) -> reqwest::RequestBuilder {
    let host = host_header(route, call);
    let mut req = (req.header("host", &host))
        .header("x-forwarded-host", &host)
        .header("x-forwarded-proto", call.scheme)
        .header("x-forwarded-port", call.port.to_string());
    if let Some(agent) = sender.agent {
        req = req
            .header(AGENT_HEADER, agent)
            .header(OLD_AGENT_HEADER, agent);
    }
    if sender.readonly {
        req = req
            .header(READONLY_HEADER, "1")
            .header(OLD_READONLY_HEADER, "1");
    }
    req
}

/// The upstream's answer read whole, its effect header taken out.
async fn answered(resp: reqwest::Response) -> Result<Forwarded, String> {
    let status = resp.status().as_u16();
    let mut effect = None;
    let mut headers = vec![];
    for (k, v) in resp.headers() {
        let value = String::from_utf8_lossy(v.as_bytes()).into_owned();
        match k.as_str() {
            EFFECT_HEADER => effect = effect_of(&value),
            OLD_EFFECT_HEADER => effect = effect.or_else(|| effect_of(&value)),
            name if is_hop(name) => {}
            name => headers.push((name.to_string(), value)),
        }
    }
    let body = resp.bytes().await.map_err(|e| error_chain(&e))?;
    Ok(Forwarded {
        status,
        headers,
        body,
        effect,
    })
}

fn effect_of(value: &str) -> Option<Effect> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read" => Some(Effect::Read),
        "write" => Some(Effect::Write),
        _ => None,
    }
}

/// An error with its causes, `: `-separated (reqwest's own message names only the URL).
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        text = format!("{text}: {s}");
        source = s.source();
    }
    text
}
