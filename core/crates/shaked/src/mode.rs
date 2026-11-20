//! Where shaked listens and which URL it hands out, and the Host names it answers. The open-source
//! shaked serves only this machine.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use reqwest::Url;

/// The address to listen on, the URL agents and callers are given, and the Host names answered.
#[derive(Debug, Clone)]
pub struct Bind {
    pub addr: SocketAddr,
    pub public_url: Url,
    pub allowed_hosts: Vec<String>,
}

const DEFAULT_PORT: u16 = 4510;
/// The Host names of this machine's loopback.
const LOOPBACK_NAMES: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];

/// The bind from settings (`get` reads one): `SHAKEDOWN_BIND` (default `127.0.0.1:4510`; the old
/// `SHAKEDOWN_PORT` still binds `127.0.0.1:<port>`), `SHAKEDOWN_PUBLIC_URL` (default
/// `http://<bind>`) and `SHAKEDOWN_ALLOWED_HOSTS` (comma-separated, added to the loopback names
/// and the public URL's host). A bind beyond loopback needs `serves_others` (a layer that
/// authenticates callers).
pub fn bind_from(
    get: impl Fn(&str) -> Option<String>,
    serves_others: bool,
) -> Result<Bind, String> {
    let set = |k: &str| {
        get(k)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let addr = addr_from(set("SHAKEDOWN_BIND"), set("SHAKEDOWN_PORT"))?;
    if !addr.ip().is_loopback() && !serves_others {
        return Err(format!(
            "SHAKEDOWN_BIND {addr}: binding beyond this machine needs a layer that authenticates callers; \
             the open-source shaked serves only this machine"
        ));
    }
    let public = set("SHAKEDOWN_PUBLIC_URL").unwrap_or_else(|| format!("http://{addr}"));
    let public_url =
        Url::parse(&public).map_err(|e| format!("SHAKEDOWN_PUBLIC_URL {public:?}: {e}"))?;
    let mut allowed_hosts = loopback_hosts();
    allowed_hosts.extend(public_url.host_str().map(str::to_string));
    let listed = set("SHAKEDOWN_ALLOWED_HOSTS").unwrap_or_default();
    let listed = listed.split(',').map(|h| h.trim().to_ascii_lowercase());
    allowed_hosts.extend(listed.filter(|h| !h.is_empty()));
    allowed_hosts.dedup();
    Ok(Bind {
        addr,
        public_url,
        allowed_hosts,
    })
}

/// `SHAKEDOWN_BIND`, else `127.0.0.1:<SHAKEDOWN_PORT>`, else `127.0.0.1:4510`.
fn addr_from(bind: Option<String>, port: Option<String>) -> Result<SocketAddr, String> {
    let loopback = |port| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    match (bind, port) {
        (Some(b), _) => b
            .parse()
            .map_err(|_| format!("SHAKEDOWN_BIND {b:?} is not host:port")),
        (None, Some(p)) => p
            .parse()
            .map(loopback)
            .map_err(|_| format!("SHAKEDOWN_PORT {p:?} is not a port number")),
        (None, None) => Ok(loopback(DEFAULT_PORT)),
    }
}

/// The name a Host header value names, without its port: `[::1]:4510` is `::1`. None when the
/// value is malformed (a port that is not a number, an unclosed bracket, a bare IPv6 address).
fn host_name(host: &str) -> Option<&str> {
    match host.strip_prefix('[') {
        Some(rest) => match rest.split_once(']')? {
            (ip, "") => Some(ip),
            (ip, port) => port.strip_prefix(':')?.parse::<u16>().ok().map(|_| ip),
        },
        None => match host.split_once(':') {
            Some((n, port)) => port.parse::<u16>().ok().map(|_| n),
            None => Some(host),
        },
    }
}

/// True when the Host header value `host` names one of `allowed` (names compared without case;
/// an IPv6 entry is written in brackets). The DNS-rebinding guard: a page on another origin can
/// resolve its own name to this machine, and its browser then sends that name.
pub fn host_allowed(host: &str, allowed: &[String]) -> bool {
    let Some(name) = host_name(host) else {
        return false;
    };
    !name.is_empty()
        && allowed
            .iter()
            .map(|a| a.trim_start_matches('[').trim_end_matches(']'))
            .any(|a| a.eq_ignore_ascii_case(name))
}

/// The loopback names, the allowed hosts of a local shaked with no settings.
pub fn loopback_hosts() -> Vec<String> {
    LOOPBACK_NAMES.iter().map(|h| h.to_string()).collect()
}

/// `Err` naming `what` where packages and code connections run only in sandboxes (a layer whose
/// policy says so).
pub fn local_only(policy: &crate::layer::Policy, what: &str) -> Result<(), String> {
    match policy.runs_packages_here {
        true => Ok(()),
        false => Err(format!(
            "{what}: this shaked runs packages only in sandboxes"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv6_binds_and_loopback_checks() {
        let get = |k: &str| (k == "SHAKEDOWN_BIND").then(|| "[::1]:4511".to_string());
        let b = bind_from(get, false).unwrap();
        assert_eq!(b.public_url.as_str(), "http://[::1]:4511/");
        let get = |k: &str| (k == "SHAKEDOWN_BIND").then(|| "10.0.0.5:4510".to_string());
        assert!(bind_from(get, false).is_err());
        let get = |k: &str| (k == "SHAKEDOWN_BIND").then(|| "nonsense".to_string());
        assert!(bind_from(get, true).unwrap_err().contains("SHAKEDOWN_BIND"));
    }
}
