//! The hosts an instance serves through shaked's proxy (`http_routes`): each `http` entry of its
//! config, its upstream the port Docker published for the compose service. shaked asks again
//! after each save, and a save that restarted a service may have moved it to a new port, so a
//! route is given only once its port accepts connections.

use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use shakedown_part::helpers::compose::Project;
use shakedown_part::proto::{HttpRoute, HttpRoutes};

use crate::config::HttpSpec;

/// How long a published port may take to accept connections.
const READY_WITHIN: Duration = Duration::from_secs(60);

/// The routes of `specs` on the running `project`, each once its port accepts connections.
pub fn routes(project: &Project, specs: &[HttpSpec]) -> anyhow::Result<HttpRoutes> {
    let mut routes = vec![];
    for s in specs {
        let at = project.port(&s.service, s.port)?;
        wait_ready(&at).with_context(|| format!("{} of {}", s.port, s.service))?;
        routes.push(HttpRoute {
            host: s.host.clone(),
            upstream: format!("http://{at}"),
            env: s.env.clone(),
            reads: s.reads.clone(),
            honours_readonly: false,
        });
    }
    Ok(HttpRoutes { routes })
}

/// Waits until `at` (`127.0.0.1:<port>`) accepts a connection, at most `READY_WITHIN`.
fn wait_ready(at: &str) -> anyhow::Result<()> {
    let addr: SocketAddr = at.parse()?;
    let since = Instant::now();
    while let Err(e) = TcpStream::connect_timeout(&addr, Duration::from_secs(1)) {
        if since.elapsed() > READY_WITHIN {
            bail!("nothing listened at {at} within 60 s: {e}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
