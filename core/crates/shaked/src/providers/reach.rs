//! Whether a model server's URL may be used here. Locally a vLLM on 127.0.0.1 or a box on the LAN
//! is the normal case; where the layer's policy forbids private model URLs (a shaked that serves
//! others), a server must be HTTPS and every address its name resolves to public: no private
//! ranges, loopback, link-local (the cloud metadata address among them) or unique-local IPv6.
//! Checked when a key is saved, when it is tested and before each call; and, on every client
//! that carries a caller's URL ([`guard`]), again at connect time (a name is looked up once, by
//! the client, and only its public addresses are dialled: no DNS rebinding between the check and
//! the connect) and at every redirect.

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
};

use reqwest::Url;

use crate::layer::Policy;

/// The checks that need no lookup: http(s), no credentials in the URL, and, when the policy
/// forbids private URLs, HTTPS and no private address literal or `localhost`.
pub fn reachable(url: &Url, policy: &Policy) -> Result<(), String> {
    reachable_as(url, policy, MODEL_SERVER)
}

/// What the checks name when the URL is a model server's.
const MODEL_SERVER: &str = "a model server";

/// [`reachable`] for the URL of `what` ("an agent", "a model server").
pub fn reachable_as(url: &Url, policy: &Policy, what: &str) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("{what}'s URL must be http or https"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!("put {what}'s key in the key field, not in its URL"));
    }
    if policy.private_model_urls {
        return Ok(());
    }
    if url.scheme() != "https" {
        return Err(format!("{what}'s URL must be https here"));
    }
    match host_of(url) {
        Some(Host::Ip(ip)) => public_as(ip, what),
        Some(Host::Name(d)) if is_local_name(d) => Err(private_refusal(what)),
        Some(Host::Name(_)) => Ok(()),
        None => Err(format!("{what}'s URL needs a host")),
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
    reach_as(url, policy, MODEL_SERVER).await
}

/// [`reach`] for the URL of `what`.
pub async fn reach_as(url: &str, policy: &Policy, what: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|e| format!("not a URL: {e}"))?;
    reachable_as(&parsed, policy, what)?;
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
        public_as(a.ip(), what)?;
        any = true;
    }
    match any {
        true => Ok(()),
        false => Err(format!("{host} resolves to no address")),
    }
}

/// The most redirects a guarded client follows.
const MAX_REDIRECTS: usize = 5;

/// How a guarded client looks a name up: the system's resolver, or a stub in tests.
pub trait Lookup: Send + Sync {
    fn lookup(
        &self,
        host: &str,
    ) -> futures_util::future::BoxFuture<'static, std::io::Result<Vec<SocketAddr>>>;
}

/// The system's resolver.
pub struct SystemLookup;

impl Lookup for SystemLookup {
    fn lookup(
        &self,
        host: &str,
    ) -> futures_util::future::BoxFuture<'static, std::io::Result<Vec<SocketAddr>>> {
        let host = host.to_string();
        Box::pin(async move { Ok(tokio::net::lookup_host((host.as_str(), 0)).await?.collect()) })
    }
}

/// A resolver that hands the client only a name's public addresses.
pub struct PublicOnly(pub Arc<dyn Lookup>);

impl reqwest::dns::Resolve for PublicOnly {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let (inner, host) = (self.0.clone(), name.as_str().to_string());
        Box::pin(async move {
            let found = inner.lookup(&host).await?;
            let kept: Vec<SocketAddr> = found.into_iter().filter(|a| public(a.ip())).collect();
            if kept.is_empty() {
                return Err(format!("{host} resolves to no public address").into());
            }
            Ok(Box::new(kept.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// `builder` for a client that carries callers' URLs: where the policy forbids private URLs, it
/// dials only public addresses, takes no proxy from the environment, and checks every redirect
/// with [`reachable`] (an address literal is never looked up, so the resolver cannot see it).
/// Unchanged where private URLs are fine (a local shaked).
pub fn guard(builder: reqwest::ClientBuilder, policy: &Policy) -> reqwest::ClientBuilder {
    guard_with(builder, policy, Arc::new(SystemLookup))
}

/// [`guard`] looking names up with `lookup`.
pub fn guard_with(
    builder: reqwest::ClientBuilder,
    policy: &Policy,
    lookup: Arc<dyn Lookup>,
) -> reqwest::ClientBuilder {
    if policy.private_model_urls {
        return builder;
    }
    let checked = *policy;
    let redirects = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error(format!("more than {MAX_REDIRECTS} redirects"));
        }
        match reachable(attempt.url(), &checked) {
            Ok(()) => attempt.follow(),
            Err(why) => attempt.error(format!("a redirect was refused: {why}")),
        }
    });
    builder
        .no_proxy()
        .dns_resolver(Arc::new(PublicOnly(lookup)))
        .redirect(redirects)
}

fn private_refusal(what: &str) -> String {
    format!("{what} must be on a public address here")
}

fn is_local_name(d: &str) -> bool {
    let d = d.trim_end_matches('.').to_ascii_lowercase();
    d == "localhost"
        || d.ends_with(".localhost")
        || d.ends_with(".internal")
        || d.ends_with(".local")
}

/// True when `ip` is a public address.
pub fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
    }
}

/// Ok when `ip` is a public address, else the refusal naming `what`.
fn public_as(ip: IpAddr, what: &str) -> Result<(), String> {
    public(ip)
        .then_some(())
        .ok_or_else(|| private_refusal(what))
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

    /// A lookup that answers every name with `addrs`.
    struct Stub(Vec<SocketAddr>);

    impl Lookup for Stub {
        fn lookup(
            &self,
            _: &str,
        ) -> futures_util::future::BoxFuture<'static, std::io::Result<Vec<SocketAddr>>> {
            let addrs = self.0.clone();
            Box::pin(async move { Ok(addrs) })
        }
    }

    /// A server on 127.0.0.1 that redirects `/hop` to `/secret` on `to` and counts the hits on
    /// `/secret`; its port and the count.
    async fn server(to: &'static str) -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
        use axum::{response::Redirect, routing::get, Router};
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let h = hits.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app =
            Router::new()
                .route(
                    "/hop",
                    get(move || async move {
                        Redirect::temporary(&format!("http://{to}:{port}/secret"))
                    }),
                )
                .route(
                    "/secret",
                    get(move || async move {
                        h.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        "metadata"
                    }),
                );
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (port, hits)
    }

    /// C2: the check before a call and the client's own lookup are one and the same: a name that
    /// answers a public address to the check and loopback to the client (DNS rebinding) is never
    /// dialled, and of a mixed answer only the public addresses are kept.
    #[tokio::test]
    async fn a_guarded_client_dials_only_public_addresses() {
        use reqwest::dns::Resolve;
        let (port, hits) = server("127.0.0.1").await;
        let to_loopback = Arc::new(Stub(vec![SocketAddr::from(([127, 0, 0, 1], port))]));
        let client = guard_with(reqwest::Client::builder(), &STRICT, to_loopback)
            .build()
            .unwrap();
        let r = client
            .get(format!("http://rebound.test:{port}/secret"))
            .send()
            .await;
        assert!(r.is_err(), "{r:?}");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0);
        let mixed = vec![
            SocketAddr::from(([10, 0, 0, 1], 0)),
            SocketAddr::from(([8, 8, 8, 8], 0)),
        ];
        let only = PublicOnly(Arc::new(Stub(mixed)));
        let kept: Vec<_> = only
            .resolve("mixed.test".parse().unwrap())
            .await
            .unwrap()
            .collect();
        assert_eq!(kept, [SocketAddr::from(([8, 8, 8, 8], 0))]);
    }

    /// C2: a guarded client checks every redirect, so a server that passed the check cannot send
    /// it on to a private address (an address literal is never looked up).
    #[tokio::test]
    async fn a_guarded_client_checks_every_redirect() {
        let (port, hits) = server("127.0.0.1").await;
        let client = guard(reqwest::Client::builder(), &STRICT).build().unwrap();
        let e = client
            .get(format!("http://127.0.0.1:{port}/hop"))
            .send()
            .await
            .unwrap_err();
        assert!(e.is_redirect(), "{e}");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0);
        let local = guard(reqwest::Client::builder(), &Policy::LOCAL)
            .build()
            .unwrap();
        let r = local
            .get(format!("http://127.0.0.1:{port}/hop"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            r.text().await.unwrap(),
            "metadata",
            "locally redirects are followed"
        );
    }

    #[tokio::test]
    async fn a_name_that_resolves_to_a_private_address_is_refused() {
        assert!(reach("https://localhost.:8443/v1", &STRICT).await.is_err());
        assert!(reach("http://localhost:8000/v1", &Policy::LOCAL)
            .await
            .is_ok());
    }
}
