//! The environment drivers and agents get in a world whose parts serve HTTP: the proxy (with a
//! ticket of their own) and the variables the parts' routes name. A world that serves none gives
//! nothing, so drivers that call providers directly work as before.

use std::collections::BTreeMap;

use crate::{app::AppState, exec::BranchCtx};

/// Hosts that never go through the proxy: shaked itself, and anything else on the machine.
pub const NO_PROXY: &str = "127.0.0.1,localhost,::1";

/// The variables for `agent` (None: the branch's driver) on `ctx`'s branch: empty when its world
/// serves no HTTP or the proxy isn't serving; else the proxy URL with a new ticket, `NO_PROXY`
/// and the hosts' variables.
pub fn env_for(state: &AppState, ctx: &BranchCtx, agent: Option<&str>) -> BTreeMap<String, String> {
    let hosts = ctx.world.hosts();
    let expires = std::time::Instant::now() + ctx.token_ttl();
    let ticket = state.proxy.tickets.issue(&ctx.branch_id, agent, expires);
    let Some(url) = (!hosts.is_empty())
        .then(|| state.proxy.url(&ticket))
        .flatten()
    else {
        return BTreeMap::new();
    };
    let mut env = BTreeMap::new();
    for k in [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "SHAKEDOWN_PROXY",
        "ARENA_PROXY",
    ] {
        env.insert(k.to_string(), url.clone());
    }
    for k in ["NO_PROXY", "no_proxy"] {
        env.insert(k.to_string(), NO_PROXY.to_string());
    }
    env.insert("NODE_USE_ENV_PROXY".into(), "1".into());
    env.extend(ca_env(state, &ctx.branch_id));
    env.extend(hosts.env());
    env
}

/// The variables that make clients trust `branch`'s CA (none, logged, when it can't be made).
fn ca_env(state: &AppState, branch: &str) -> BTreeMap<String, String> {
    let ca = match state.proxy.cas.for_branch(branch) {
        Ok(ca) => ca,
        Err(e) => {
            eprintln!("proxy: no CA for {branch}: {e:#}");
            return BTreeMap::new();
        }
    };
    let pem = ca.pem_path().to_string_lossy().into_owned();
    let vars = CA_VARS.iter().map(|k| (k.to_string(), pem.clone()));
    vars.collect()
}

/// The variables clients read the CAs they trust from.
const CA_VARS: [&str; 5] = [
    "SSL_CERT_FILE",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
    "NODE_EXTRA_CA_CERTS",
    "GIT_SSL_CAINFO",
];
