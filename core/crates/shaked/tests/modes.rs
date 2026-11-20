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
    assert_eq!(b.allowed_hosts, shaked::mode::loopback_hosts(), "URLs only");
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
        true,
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

/// C1: a tunnel to a local shaked forwards the tunnel's own Host. A tokenless request is the
/// person at this machine, so `/api` must never answer a name beyond loopback locally, whatever
/// the public URL or the allowed hosts say.
#[test]
fn a_local_shaked_answers_the_api_on_loopback_names_only() {
    let e = bind(&[("SHAKEDOWN_ALLOWED_HOSTS", "x.ngrok.app")], false).unwrap_err();
    assert!(e.contains("SHAKEDOWN_ALLOWED_HOSTS"), "{e}");
    let served = bind(&[("SHAKEDOWN_PUBLIC_URL", "https://sd.example")], true).unwrap();
    assert!(served.allowed_hosts.iter().any(|h| h == "sd.example"));
    run(async {
        let fx = AppFx::idle("modes", "tunnel")
            .config(|c| {
                c.public_url = "https://x.ngrok.app".into();
                c.allowed_hosts = vec!["x.ngrok.app".into(), "localhost".into()];
            })
            .start()
            .await;
        let url = format!("{}/api/runs", fx.base);
        let through = fx.http.get(&url).header("host", "x.ngrok.app");
        let (status, v) = support::app::send(through).await;
        assert_eq!(status, 403, "{v}");
        assert_eq!(fx.get("/api/runs").await.0, 200, "loopback still answers");
    })
}

/// A Code connection that would run `true` here.
fn connection() -> shaked::store::Connection {
    shaked::store::Connection {
        id: "c".into(),
        name: "C".into(),
        kind: "code".into(),
        command: vec!["true".into()],
        workdir: None,
        env: BTreeMap::new(),
        created_at: chrono::Utc::now(),
    }
}

/// A built-in run of task 27 whose agent cannot reach a model: the answer to `POST /api/runs`.
async fn post_run(fx: &AppFx) -> (u16, serde_json::Value) {
    let agent = serde_json::json!({"id": "planner", "name": "Planner", "kind": "open",
                                   "model": "m", "prompt": "Poke the box."});
    let team = serde_json::json!({"id": "o", "name": "Open", "agents": [agent]});
    fx.post(
        "/api/runs",
        serde_json::json!({"task_id": "27", "team": team}),
    )
    .await
}

/// [`post_run`], ended: its branch.
async fn ended_run(fx: &AppFx) -> String {
    let (status, v) = post_run(fx).await;
    assert_eq!(status, 200, "{v}");
    let branch = v["branch_id"].as_str().unwrap().to_string();
    for _ in 0..100 {
        let (_, b) = fx.get(&format!("/api/branches/{branch}")).await;
        if b["branch"]["status"] != "running" && b["status"] != "running" {
            return branch;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the run never ended");
}

const SANDBOXES: &str = "this shaked runs packages only in sandboxes";

/// Fork `parent` at its last event and launch a Code connection's driver here: both refused.
async fn fork_and_driver_are_refused(fx: &AppFx, parent: &str) {
    let events = support::records(&fx.state.store).events(parent).await;
    let at = events.unwrap().last().unwrap().seq;
    let forked = shaked::fork::fork(&fx.state, parent, at, Default::default()).await;
    let e = forked.expect_err("a fork started a world here");
    assert!(format!("{e:#}").contains(SANDBOXES), "{e:#}");
    let ids = shaked::driver::RunIds {
        run_id: "r",
        branch_id: "b",
        task_id: "27",
        fork_of: None,
    };
    let launched = shaked::driver::launch(&fx.state, &connection(), ids).await;
    let e = launched.err().expect("a driver launched here");
    assert!(format!("{e:#}").contains(SANDBOXES), "{e:#}");
}

/// I8: under a layer whose packages run only in sandboxes, nothing starts a world's parts or a
/// driver on this machine: not the machine's world routes, a new run, a fork of a run made
/// before, nor a Code connection's driver.
#[test]
fn a_layer_that_runs_no_packages_here_starts_no_world_driver_or_fork() {
    run(async {
        let spy = shaked::harness::SpyLayer::new();
        let fx = AppFx::idle("modes", "sandboxed")
            .layer(std::sync::Arc::new(spy.clone()))
            .start()
            .await;
        let parent = ended_run(&fx).await;
        spy.sandbox_packages();
        let (status, v) = fx.post("/api/worlds/x/use", serde_json::json!({})).await;
        assert_eq!((status, v["error"].as_str()), (403, Some(SANDBOXES)), "{v}");
        let (status, v) = post_run(&fx).await;
        assert_ne!(status, 200, "{v}");
        assert!(v["error"].as_str().unwrap_or("").contains(SANDBOXES), "{v}");
        fork_and_driver_are_refused(&fx, &parent).await;
    })
}
