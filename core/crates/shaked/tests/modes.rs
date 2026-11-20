//! Where shaked listens and the URLs it hands agents; a bind beyond this machine is refused.

use std::collections::BTreeMap;

use shaked::{app::Config, mode::bind_from};
use support::{run, AppFx};

mod support;

/// `bind_from` over the given settings only.
fn bind(vars: &[(&str, &str)], serves_others: bool) -> Result<shaked::mode::Bind, String> {
    let vars: BTreeMap<String, String> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    bind_from(|k| vars.get(k).cloned(), serves_others)
}

#[test]
fn defaults_are_todays() {
    let b = bind(&[], false).unwrap();
    assert_eq!(b.addr.to_string(), "127.0.0.1:4510");
    assert_eq!(b.public_url.as_str(), "http://127.0.0.1:4510/");
    for h in ["127.0.0.1", "localhost", "[::1]"] {
        assert!(
            b.allowed_hosts.iter().any(|a| a == h),
            "{h} in {:?}",
            b.allowed_hosts
        );
    }
}

#[test]
fn old_port_setting_still_binds() {
    let b = bind(&[("SHAKEDOWN_PORT", "4600")], false).unwrap();
    assert_eq!(b.addr.to_string(), "127.0.0.1:4600");
    assert_eq!(b.public_url.as_str(), "http://127.0.0.1:4600/");
}

#[test]
fn a_public_bind_is_refused() {
    let e = bind(&[("SHAKEDOWN_BIND", "0.0.0.0:4510")], false).unwrap_err();
    assert!(e.contains("serves only this machine"), "{e}");
    let b = bind(&[("SHAKEDOWN_BIND", "0.0.0.0:4510")], true).unwrap();
    assert_eq!(
        b.addr.to_string(),
        "0.0.0.0:4510",
        "a layer that authenticates callers may"
    );
}

#[test]
fn me_is_personal() {
    run(async {
        let fx = AppFx::idle("modes", "me").start().await;
        let (status, v) = fx.get("/api/me").await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(
            v,
            serde_json::json!({"mode": "local", "user": {"name": "You"}, "features": []})
        );
    })
}

#[test]
fn agent_urls_derive_from_the_public_url() {
    let b = bind(&[("SHAKEDOWN_PUBLIC_URL", "https://sd.example")], false).unwrap();
    assert!(
        b.allowed_hosts.iter().any(|h| h == "sd.example"),
        "{:?}",
        b.allowed_hosts
    );
    let mut c = Config::new();
    c.self_base = "http://127.0.0.1:4510".into();
    c.agent_base = String::new();
    c.public_url = b.public_url.to_string();
    let e = shaked::external::endpoints(c.public_base(), "b1", "a", BTreeMap::new());
    assert!(
        e.mcp_url.starts_with("https://sd.example/mcp/"),
        "{}",
        e.mcp_url
    );
    assert!(
        e.gateway_url.starts_with("https://sd.example/gateway/"),
        "{}",
        e.gateway_url
    );
    assert!(
        c.http_agent_base().starts_with("https://sd.example"),
        "{}",
        c.http_agent_base()
    );
}

/// Every case of the loopback guard's own test, plus listed hosts.
const ALLOWED: &[&str] = &[
    "127.0.0.1",
    "127.0.0.1:4510",
    "localhost",
    "LOCALHOST:80",
    "[::1]",
    "[::1]:4510",
    "sd.example",
    "SD.EXAMPLE:443",
    "other.example",
];
const REFUSED: &[&str] = &[
    "evil.example",
    "127.0.0.1.evil.example",
    "localhost.evil.com:80",
    "127.0.0.1:abc",
    "[::2]",
    "",
    "::1",
    "sd.example.evil.test",
];

#[test]
fn allowed_hosts_keep_the_dns_rebinding_guard() {
    use shaked::mode::host_allowed as ok;
    let b = bind(
        &[("SHAKEDOWN_ALLOWED_HOSTS", "sd.example, Other.Example ")],
        false,
    )
    .unwrap();
    for h in ALLOWED {
        assert!(ok(h, &b.allowed_hosts), "{h}");
    }
    for h in REFUSED {
        assert!(!ok(h, &b.allowed_hosts), "{h}");
    }
}

#[test]
fn local_mode_still_answers_the_api() {
    run(async {
        let fx = AppFx::idle("modes", "local").start().await;
        assert_eq!(fx.get("/api/runs").await.0, 200);
    })
}
