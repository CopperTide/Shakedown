//! `CONNECT` to a host a world's part serves: the proxy answers 200, terminates TLS with a leaf
//! of the branch's CA for that host, and runs each request inside as an `https` step. Other hosts,
//! and a served host on a port other than 443, are refused and recorded.

use std::{convert::Infallible, sync::Arc};

use hyper::{body::Incoming, server::conn::http1, service::service_fn, Request};
use hyper_util::rt::TokioIo;
use tokio_rustls::{rustls::ServerConfig, TlsAcceptor};

use super::request::{self, Answer};
use crate::{
    app::AppState,
    exec::{BranchCtx, HttpCall, ToolExec, NOT_RUNNING},
};

/// The one port a served host takes HTTPS on.
const HTTPS_PORT: u16 = 443;

/// Who a tunnel is for: the branch, the ticket's agent and the host.
#[derive(Clone)]
struct Tunnel {
    state: Arc<AppState>,
    branch: Arc<BranchCtx>,
    agent: Option<String>,
    host: String,
}

/// Answers a `CONNECT` of `agent` on `branch`.
pub async fn connect(
    state: Arc<AppState>,
    branch: Arc<BranchCtx>,
    agent: Option<String>,
    req: Request<Incoming>,
) -> Answer {
    let Some(authority) = req.uri().authority().cloned() else {
        return request::text(400, "a CONNECT names host:port");
    };
    let host = authority.host().to_ascii_lowercase();
    let port = authority.port_u16().unwrap_or(HTTPS_PORT);
    if let Some(why) = refusal(&branch, &host, port) {
        let exec = ToolExec::new(state.store.clone());
        let call = connect_call(&host, port, &req);
        let answer = exec.http_refusal(&branch, agent.as_deref(), call, (403, &why));
        return request::to_response(answer.await);
    }
    let config = state.proxy.cas.for_branch(&branch.branch_id);
    let config = match config.and_then(|ca| ca.server_config(&host)) {
        Ok(c) => c,
        Err(e) => return request::text(502, &format!("no TLS for {host}: {e:#}")),
    };
    let tunnel = Tunnel {
        state,
        branch,
        agent,
        host,
    };
    tokio::spawn(tunnel.serve(config, req));
    request::text(200, "")
}

/// Why a `CONNECT` to `host:port` is refused: no part serves the host, or not on 443.
fn refusal(branch: &BranchCtx, host: &str, port: u16) -> Option<String> {
    if branch.world.hosts().route(host).is_none() {
        return Some(format!("the host {host} is not part of this world"));
    }
    (port != HTTPS_PORT).then(|| format!("the host {host} takes HTTPS on port 443 only"))
}

/// The `CONNECT` itself, as a recorded request.
fn connect_call(host: &str, port: u16, req: &Request<Incoming>) -> HttpCall {
    HttpCall {
        scheme: "https",
        method: "CONNECT".into(),
        host: host.to_string(),
        port,
        path_and_query: String::new(),
        headers: request::header_pairs(req.headers()),
        body: bytes::Bytes::new(),
    }
}

impl Tunnel {
    /// Once the client's connection is handed over: TLS as `host`, then HTTP/1.1 inside.
    async fn serve(self, config: Arc<ServerConfig>, req: Request<Incoming>) {
        let Ok(upgraded) = hyper::upgrade::on(req).await else {
            return;
        };
        let accepted = TlsAcceptor::from(config).accept(TokioIo::new(upgraded));
        // A client that doesn't trust the world's CA ends here: no request reached a part.
        let Ok(tls) = accepted.await else {
            return;
        };
        let service = service_fn(move |inner| {
            let tunnel = self.clone();
            async move { Ok::<_, Infallible>(tunnel.inside(inner).await) }
        });
        let conn = http1::Builder::new().serve_connection(TokioIo::new(tls), service);
        let _ = conn.await;
    }

    /// One request inside the tunnel, run as an `https` step.
    async fn inside(&self, req: Request<Incoming>) -> Answer {
        if !self.state.live.contains(&self.branch.branch_id) {
            return request::text(409, NOT_RUNNING);
        }
        let read = request::read_call(req, "https", self.host.clone(), HTTPS_PORT).await;
        let exec = ToolExec::new(self.state.store.clone());
        super::run_read(&exec, &self.branch, self.agent.as_deref(), read).await
    }
}
