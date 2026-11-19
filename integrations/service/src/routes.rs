//! The hosts an instance serves through arenad's proxy (`http_routes`): each `http` entry of its
//! config, its upstream the port Docker published for the compose service.

use arena_part::helpers::compose::Project;
use arena_part::proto::{HttpRoute, HttpRoutes};

use crate::config::HttpSpec;

/// The routes of `specs` on the running `project`.
pub fn routes(project: &Project, specs: &[HttpSpec]) -> anyhow::Result<HttpRoutes> {
    let mut routes = vec![];
    for s in specs {
        let at = project.port(&s.service, s.port)?;
        routes.push(HttpRoute {
            host: s.host.clone(),
            upstream: format!("http://{at}"),
            env: s.env.clone(),
            reads: s.reads.clone(),
        });
    }
    Ok(HttpRoutes { routes })
}
