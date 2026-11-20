//! Where shaked listens, the URLs it hands agents, and hosted mode's refusals.

use std::collections::BTreeMap;

use shaked::{
    app::Config,
    driver::{self, RunIds},
    mode::{bind_from, Mode, HOSTED_REFUSAL},
    store::Connection,
};
use support::{run, AppFx};

mod support;

/// `bind_from` over the given settings only.
fn bind(vars: &[(&str, &str)], hosted: bool) -> Result<shaked::mode::Bind, String> {
    let vars: BTreeMap<String, String> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    bind_from(|k| vars.get(k).cloned(), hosted)
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
fn a_public_bind_needs_hosted() {
    let err = bind(&[("SHAKEDOWN_BIND", "0.0.0.0:4510")], false).unwrap_err();
    assert!(err.contains("--hosted"), "{err}");
    let b = bind(&[("SHAKEDOWN_BIND", "0.0.0.0:4510")], true).unwrap();
    assert_eq!(b.addr.to_string(), "0.0.0.0:4510");
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

fn connection() -> Connection {
    Connection {
        id: "c1".into(),
        name: "driver".into(),
        kind: "code".into(),
        command: vec!["true".into()],
        workdir: None,
        env: BTreeMap::new(),
        created_at: chrono::Utc::now(),
    }
}

#[test]
fn hosted_refuses_every_api_call_and_part_launch() {
    run(async {
        let fx = AppFx::idle("modes", "hosted")
            .config(|c| c.mode = Mode::Hosted)
            .start()
            .await;
        let (status, v) = fx.get("/api/runs").await;
        assert_eq!(status, 401, "{v}");
        assert_eq!(
            v["error"],
            "sign in through the gateway or send an API token"
        );
        assert_eq!(fx.post("/api/runs", serde_json::json!({})).await.0, 401);
        let (status, v) = fx.post("/api/worlds/x/use", serde_json::json!({})).await;
        assert_eq!(status, 403, "{v}");
        assert_eq!(v["error"], HOSTED_REFUSAL);
        let ids = RunIds {
            run_id: "r",
            branch_id: "b",
            task_id: "t",
            fork_of: None,
        };
        match driver::launch(&fx.state, &connection(), ids).await {
            Ok(_) => panic!("a hosted shaked launched a Code connection"),
            Err(e) => assert!(format!("{e:#}").contains(HOSTED_REFUSAL), "{e:#}"),
        }
        assert!(shaked::mode::local_only(&fx.state.config, "starting a world").is_err());
    })
}

#[test]
fn local_mode_still_answers_the_api() {
    run(async {
        let fx = AppFx::idle("modes", "local").start().await;
        assert_eq!(fx.get("/api/runs").await.0, 200);
        assert!(shaked::mode::local_only(&fx.state.config, "starting a world").is_ok());
    })
}
