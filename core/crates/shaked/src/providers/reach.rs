//! Whether a model server's URL may be used here. Locally a vLLM on 127.0.0.1 or a box on the LAN
//! is the normal case; where the layer's policy forbids private model URLs (a shaked that serves
//! others), a server must be HTTPS and every address its name resolves to public: no private
//! ranges, loopback, link-local (the cloud metadata address among them) or unique-local IPv6.
//! Checked when a key is saved, when it is tested and before each call.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use reqwest::Url;

use crate::layer::Policy;

/// The checks that need no lookup: http(s), no credentials in the URL, and, when the policy
/// forbids private URLs, HTTPS and no private address literal or `localhost`.
pub fn reachable(url: &Url, policy: &Policy) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err("a model server's URL must be http or https".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("put the server's key in the key field, not in its URL".into());
    }
    if policy.private_model_urls {
        return Ok(());
    }
    if url.scheme() != "https" {
        return Err("a model server's URL must be https here".into());
    }
    match host_of(url) {
        Some(Host::Ip(ip)) => public(ip),
        Some(Host::Name(d)) if is_local_name(d) => Err(private_refusal()),
        Some(Host::Name(_)) => Ok(()),
        None => Err("a model server's URL needs a host".into()),
    }
}

enum Host<'a> {
    Ip(IpAddr),
    Name(&'a str),
}

/// `url`'s host: an address literal (IPv6 without its brackets) or a name.
fn host_of(url: &Url) -> Option<Host<'_>> {
    let h = url.host_str().filter(|h| !h.is_empty())?;
    let bare = h.trim_start_matches('[').trim_end_matches(']');
    Some(match bare.parse::<IpAddr>() {
        Ok(ip) => Host::Ip(ip),
        Err(_) => Host::Name(h),
    })
}

/// [`reachable`], then (when the policy forbids private URLs) every address the host resolves to
/// is public.
pub async fn reach(url: &str, policy: &Policy) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|e| format!("not a URL: {e}"))?;
    reachable(&parsed, policy)?;
    let Some(Host::Name(host)) = host_of(&parsed) else {
        return Ok(());
    };
    if policy.private_model_urls {
        return Ok(());
    }
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| format!("cannot resolve {host}"))?;
    let mut any = false;
    for a in addrs {
        public(a.ip())?;
        any = true;
    }
    match any {
        true => Ok(()),
        false => Err(format!("{host} resolves to no address")),
    }
}

fn private_refusal() -> String {
    "a model server must be on a public address here".into()
}

fn is_local_name(d: &str) -> bool {
    let d = d.trim_end_matches('.').to_ascii_lowercase();
    d == "localhost"
        || d.ends_with(".localhost")
        || d.ends_with(".internal")
        || d.ends_with(".local")
}

/// Ok when `ip` is a public address.
fn public(ip: IpAddr) -> Result<(), String> {
    let ok = match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
    };
    ok.then_some(()).ok_or_else(private_refusal)
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    let shared = a == 100 && (64..128).contains(&b); // 100.64.0.0/10, carrier-grade NAT
    let benchmarking = a == 198 && (b == 18 || b == 19);
    let reserved = a >= 240 || a == 0;
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || shared
        || benchmarking
        || reserved)
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    let s = ip.segments();
    let nat64 = s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0];
    if nat64 {
        let o = ip.octets();
        return public_v4(Ipv4Addr::new(o[12], o[13], o[14], o[15]));
    }
    let unique_local = (s[0] & 0xfe00) == 0xfc00;
    let link_local = (s[0] & 0xffc0) == 0xfe80;
    let site_local = (s[0] & 0xffc0) == 0xfec0;
    let documentation = s[0] == 0x2001 && s[1] == 0x0db8;
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || unique_local
        || link_local
        || site_local
        || documentation)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRICT: Policy = Policy {
        serves_others: true,
        private_model_urls: false,
        runs_packages_here: false,
    };

    fn ok(url: &str, p: &Policy) -> bool {
        reachable(&Url::parse(url).unwrap(), p).is_ok()
    }

    #[test]
    fn private_servers_are_fine_locally() {
        for url in [
            "http://127.0.0.1:8000/v1",
            "http://10.0.0.5/v1",
            "http://localhost:11434/v1",
        ] {
            assert!(ok(url, &Policy::LOCAL), "{url}");
        }
        assert!(!ok("ftp://127.0.0.1/", &Policy::LOCAL));
        assert!(
            !ok("http://user:pw@127.0.0.1/", &Policy::LOCAL),
            "no credentials in URLs"
        );
    }

    #[test]
    fn where_the_policy_forbids_them_only_public_https_passes() {
        for url in [
            "http://api.example.com/v1",
            "https://127.0.0.1/v1",
            "https://10.0.0.5/v1",
            "https://172.16.3.4/v1",
            "https://192.168.1.2/v1",
            "https://169.254.169.254/latest",
            "https://100.64.0.1/v1",
            "https://0.0.0.0/v1",
            "https://[::1]/v1",
            "https://[fc00::1]/v1",
            "https://[fd12::1]/v1",
            "https://[fe80::1]/v1",
            "https://[::ffff:10.0.0.1]/v1",
            "https://[64:ff9b::a00:1]/v1",
            "https://localhost/v1",
            "https://box.internal/v1",
        ] {
            assert!(!ok(url, &STRICT), "{url}");
        }
        for url in [
            "https://8.8.8.8/v1",
            "https://[2606:4700::1]/v1",
            "https://api.example.com/v1",
        ] {
            assert!(ok(url, &STRICT), "{url}");
        }
    }

    #[tokio::test]
    async fn a_name_that_resolves_to_a_private_address_is_refused() {
        assert!(reach("https://localhost.:8443/v1", &STRICT).await.is_err());
        assert!(reach("http://localhost:8000/v1", &Policy::LOCAL)
            .await
            .is_ok());
    }
}
