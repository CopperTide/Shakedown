//! arenad's recording proxy: drivers' and agents' own HTTP clients (`git`, `gh`, `curl`, SDKs)
//! reach the hosts a world's parts serve through it, and each request is recorded as an `http`
//! step of the part serving the host (see `exec::http`). One listener on 127.0.0.1 serves every
//! branch: the ticket in the proxy URL says which branch (and agent) a request is from.

pub mod env;
pub mod forward;
pub mod record;
pub mod request;
pub mod tickets;

use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, OnceLock},
};

use hyper::{body::Incoming, server::conn::http1, service::service_fn, Method, Request};
use hyper_util::rt::TokioIo;

use crate::{
    app::AppState,
    exec::{ToolExec, NOT_RUNNING},
};
use request::{Answer, Read};
pub use tickets::{Holder, Tickets};

/// The proxy's tickets and where it listens.
#[derive(Debug, Default)]
pub struct Proxy {
    pub tickets: Tickets,
    addr: OnceLock<SocketAddr>,
}

impl Proxy {
    /// Where the proxy listens (None until it serves).
    pub fn addr(&self) -> Option<SocketAddr> {
        self.addr.get().copied()
    }

    /// Records that the proxy listens at `addr` (`serve` does it; the first address stays).
    pub fn listening(&self, addr: SocketAddr) {
        let _ = self.addr.set(addr);
    }

    /// The proxy URL that carries `ticket` (`http://arena:<ticket>@127.0.0.1:<port>`).
    pub fn url(&self, ticket: &str) -> Option<String> {
        let port = self.addr()?.port();
        Some(format!(
            "http://{}:{ticket}@127.0.0.1:{port}",
            tickets::PROXY_USER
        ))
    }
}

/// Serves the proxy on `listener` (127.0.0.1) until the process ends.
pub async fn serve(state: Arc<AppState>, listener: tokio::net::TcpListener) {
    if let Ok(addr) = listener.local_addr() {
        state.proxy.listening(addr);
    }
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        tokio::spawn(serve_connection(state.clone(), stream));
    }
}

async fn serve_connection(state: Arc<AppState>, stream: tokio::net::TcpStream) {
    let service = service_fn(move |req| {
        let state = state.clone();
        async move { Ok::<_, Infallible>(handle(state, req).await) }
    });
    let conn = http1::Builder::new().serve_connection(TokioIo::new(stream), service);
    // A client that goes away mid-request is no error of the proxy's.
    let _ = conn.with_upgrades().await;
}

/// One request: its ticket checked, its branch running, then run as a step.
async fn handle(state: Arc<AppState>, req: Request<Incoming>) -> Answer {
    let Some(holder) = request::holder(&state, &req) else {
        return request::ticket_wanted();
    };
    let Some(branch) = state.live.get(&holder.branch) else {
        return request::text(409, NOT_RUNNING);
    };
    if req.method() == Method::CONNECT {
        return request::text(400, "this proxy does not tunnel yet");
    }
    let exec = ToolExec::new(state.store.clone());
    let agent = holder.agent.as_deref();
    let answer = match request::read_plain(req).await {
        Read::Answer(a) => return a,
        Read::Call(call) => exec.http(&branch, agent, call).await,
        Read::TooLarge(call) => {
            let refused = (413, request::TOO_LARGE);
            exec.http_refusal(&branch, agent, call, refused).await
        }
    };
    request::to_response(answer)
}
