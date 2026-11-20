//! Reading a request the proxy got: whose ticket it carries, and the call it makes.

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{body::Incoming, header, Request, Response};

use super::{
    record::MAX_BODY,
    tickets::{ticket_of, Standing},
};
use crate::{
    app::AppState,
    exec::{HttpAnswer, HttpCall},
};

/// What the proxy answers.
pub type Answer = Response<Full<Bytes>>;

const TICKET_WANTED: &str = "this proxy wants the ticket in the proxy URL Shakedown gave";
const ABSOLUTE_WANTED: &str = "this is Shakedown's proxy: send absolute URLs";
/// The error of a request whose body is over `MAX_BODY`.
pub const TOO_LARGE: &str = "the request body is over 256 MiB";

/// `a` as a hyper response.
pub fn to_response(a: HttpAnswer) -> Answer {
    let mut r = Response::new(Full::new(a.body));
    *r.status_mut() =
        hyper::StatusCode::from_u16(a.status).unwrap_or(hyper::StatusCode::BAD_GATEWAY);
    let headers = r.headers_mut();
    for (k, v) in a.headers.iter().filter(|(k, _)| !super::forward::is_hop(k)) {
        let (Ok(k), Ok(v)) = (
            header::HeaderName::try_from(k),
            header::HeaderValue::try_from(v),
        ) else {
            continue;
        };
        headers.append(k, v);
    }
    r
}

/// A plain-text answer of the proxy's own.
pub fn text(status: u16, text: &str) -> Answer {
    to_response(HttpAnswer::text(status, text))
}

/// 407: the request has no ticket shaked gave.
pub fn ticket_wanted() -> Answer {
    let mut r = text(407, TICKET_WANTED);
    let realm = header::HeaderValue::from_static("Basic realm=\"shakedown\"");
    r.headers_mut().insert(header::PROXY_AUTHENTICATE, realm);
    r
}

/// 410: the request's ticket was its branch's, and the branch has ended.
pub fn branch_ended() -> Answer {
    text(410, BRANCH_ENDED)
}

const BRANCH_ENDED: &str =
    "the branch this proxy URL was for has ended; a new run gives a new proxy URL";

/// What the request's ticket stands for (Unknown without one).
pub fn standing<B>(state: &AppState, req: &Request<B>) -> Standing {
    let auth = req.headers().get(header::PROXY_AUTHORIZATION);
    match ticket_of(auth.and_then(|v| v.to_str().ok())) {
        Some(ticket) => state.proxy.tickets.standing(&ticket),
        None => Standing::Unknown,
    }
}

/// A request read: the call, the call whose body was too large (with no body), or the proxy's
/// own answer.
pub enum Read {
    Call(HttpCall),
    TooLarge(HttpCall),
    Answer(Answer),
}

/// The call a plain-HTTP request makes: it must name an absolute `http` URL.
pub async fn read_plain(req: Request<Incoming>) -> Read {
    let uri = req.uri();
    let (Some("http"), Some(authority)) = (uri.scheme_str(), uri.authority()) else {
        return Read::Answer(text(400, ABSOLUTE_WANTED));
    };
    let (host, port) = (authority.host().to_ascii_lowercase(), authority.port_u16());
    read_call(req, "http", host, port.unwrap_or(80)).await
}

/// The call `req` makes on `host:port` over `scheme`, its body read whole (up to `MAX_BODY`).
pub async fn read_call(
    req: Request<Incoming>,
    scheme: &'static str,
    host: String,
    port: u16,
) -> Read {
    let (head, body) = req.into_parts();
    let headers = header_pairs(&head.headers);
    let path_and_query = head.uri.path_and_query().map_or("/", |p| p.as_str());
    let mut call = HttpCall {
        scheme,
        method: head.method.to_string(),
        host,
        port,
        path_and_query: path_and_query.to_string(),
        headers,
        body: Bytes::new(),
    };
    match Limited::new(body, MAX_BODY).collect().await {
        Ok(b) => call.body = b.to_bytes(),
        Err(e) if e.is::<http_body_util::LengthLimitError>() => return Read::TooLarge(call),
        Err(e) => {
            return Read::Answer(text(
                400,
                &format!("the request body could not be read: {e}"),
            ))
        }
    }
    Read::Call(call)
}

/// Each header as a `(name, value)` pair, in order (a value that is no UTF-8 read lossily).
pub fn header_pairs(h: &header::HeaderMap) -> Vec<(String, String)> {
    let pair = |(k, v): (&header::HeaderName, &header::HeaderValue)| {
        (
            k.to_string(),
            String::from_utf8_lossy(v.as_bytes()).into_owned(),
        )
    };
    h.iter().map(pair).collect()
}
