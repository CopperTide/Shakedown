//! The live package (`live-service`): a world clock file for the containers, deploys as a world
//! tool with the images kept in saves, probes as docs and watches as events, on the fake `docker`.
mod support;

use std::path::PathBuf;
use std::time::Duration;

use arena_part::proto::*;
use arena_part::Part;
use arena_service::Service;
use serde_json::{json, Value};
use std::fs;
use support::*;

const EPOCH: i64 = 1_773_122_400_000;

fn images() -> Value {
    json!({"prod-orders": "acme/orders:seed", "prod-db": "postgres:17"})
}

/// A live config with `images`, `extra` env for the fake and `more` top-level fields.
fn live_config(f: &Fake, extra: Value, more: Value) -> Value {
    let mut c = config_with(f, json!([]), json!({"live": true, "images": images()}));
    c["env"]
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    c.as_object_mut()
        .unwrap()
        .extend(more.as_object().unwrap().clone());
    c
}

fn live_start(id: &str, config: Value) -> StartParams {
    StartParams {
        world_time: Some(WorldTimeParams {
            epoch_ms: EPOCH,
            speed: 1.0,
        }),
        ..start(id, config)
    }
}

async fn advance(p: &Service, id: &str, clock_ms: i64) -> Advanced {
    let at = AdvanceParams {
        instance: id.into(),
        clock_ms,
    };
    p.advance(at).await.unwrap()
}

async fn call(p: &Service, id: &str, name: &str, args: Value) -> ToolOutput {
    let params = CallToolParams {
        instance: id.into(),
        name: name.into(),
        args,
        clock_ms: 0,
    };
    p.call_tool(params).await.unwrap()
}

/// Advances `id` a second at a time (from 1 s) until an event of `kind`, for at most 10 s.
async fn advance_until_event(p: &Service, id: &str, kind: &str) -> WorldEvent {
    for n in 1..=100 {
        let adv = advance(p, id, n * 1000).await;
        if let Some(ev) = adv.events.into_iter().find(|e| e.kind == kind) {
            return ev;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("no {kind} event within 10 s")
}

/// The clock folder the fake saw in the compose environment.
fn clock_dir(f: &Fake) -> PathBuf {
    let log = f.log();
    let var = log
        .split_whitespace()
        .find_map(|w| w.strip_prefix("SHAKEDOWN_CLOCK_DIR="))
        .unwrap_or_else(|| panic!("no SHAKEDOWN_CLOCK_DIR in {log}"));
    PathBuf::from(var)
}

fn read_json(path: PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap()
}

fn instance(id: &str) -> Target {
    Target::Instance {
        instance: id.into(),
    }
}

async fn inspect(p: &Service, target: Target, keys: &[&str]) -> serde_json::Map<String, Value> {
    let keys = keys.iter().map(|k| k.to_string()).collect();
    p.inspect(InspectParams { target, keys })
        .await
        .unwrap()
        .docs
}

#[tokio::test]
async fn the_live_package_uses_the_clock_and_has_the_deploy_tool() {
    let d = Fake::new("livedescribe")
        .live_part()
        .describe()
        .await
        .unwrap();
    assert_eq!(d.name, "live-service");
    assert!(d.uses_clock);
    let deploy = d.tools.iter().find(|t| t.name == "service_deploy");
    assert_eq!(deploy.map(|t| t.effect), Some(Effect::Write));
    assert_eq!(d.save_at, SaveAt::Points);
}

#[tokio::test]
async fn the_plain_package_refuses_what_needs_the_clock() {
    let f = Fake::new("plainlive");
    for more in [
        json!({"live": true}),
        json!({"probes": [{"key": "p", "service": "s", "port": 1, "path": "/"}],
                 "watches": [{"probe": "p", "every_ms": 1000, "items": "", "id": "/id", "on": "a", "off": "b"}]}),
    ] {
        let mut c = config_with(&f, json!([]), json!({}));
        c.as_object_mut()
            .unwrap()
            .extend(more.as_object().unwrap().clone());
        let e = f.part().start(start("a", c)).await.unwrap_err();
        assert_eq!(e.code, -32602, "{more}");
        assert!(e.message.contains("live-service"), "{}", e.message);
    }
    assert!(f.log().is_empty());
}

#[tokio::test]
async fn a_live_service_writes_the_clock_file_on_advance() {
    let f = Fake::new("clock");
    let p = f.live_part();
    p.start(live_start("a", live_config(&f, json!({}), json!({}))))
        .await
        .unwrap();
    let dir = clock_dir(&f);
    assert_eq!(read_json(dir.join("clock.json"))["world_ms"], json!(EPOCH));
    let adv = advance(&p, "a", 5_000).await;
    let v = read_json(dir.join("clock.json"));
    assert_eq!(
        (&v["clock_ms"], &v["world_ms"]),
        (&json!(5_000), &json!(1_773_122_405_000i64))
    );
    assert_eq!((&v["epoch_ms"], &v["speed"]), (&json!(EPOCH), &json!(1.0)));
    assert!(v["wall_ms"].as_i64().unwrap() > EPOCH);
    assert!(adv.changed);
    p.stop(IdParams {
        instance: "a".into(),
    })
    .await
    .unwrap();
    assert!(!dir.exists(), "stop removes the clock folder");
}

#[tokio::test]
async fn images_reach_compose_as_image_variables() {
    let f = Fake::new("imagevars");
    let p = f.live_part();
    p.start(live_start("a", live_config(&f, json!({}), json!({}))))
        .await
        .unwrap();
    let log = f.log();
    assert!(
        log.contains("IMAGE_PROD_ORDERS=acme/orders:seed")
            && log.contains("IMAGE_PROD_DB=postgres:17"),
        "{log}"
    );
}

#[tokio::test]
async fn deploy_rolls_out_in_the_background_and_reports_deployed() {
    let f = Fake::new("deploy");
    let p = f.live_part();
    p.start(live_start("a", live_config(&f, json!({}), json!({}))))
        .await
        .unwrap();
    let args =
        json!({"service": "prod-orders", "image": "acme/orders:abc", "sha": "abc", "ref": "j1"});
    let out = call(&p, "a", "service_deploy", args).await;
    assert_eq!(out.result.unwrap(), json!({"accepted": true}));
    let ev = advance_until_event(&p, "a", "deployed").await;
    assert_eq!(ev.data["by"], "pipeline");
    let result = &ev.data["result"];
    assert_eq!(result["ok"], true, "{ev:?}");
    let said = ["job_id", "service", "image", "sha"].map(|k| result[k].clone());
    assert_eq!(
        said,
        ["j1", "prod-orders", "acme/orders:abc", "abc"].map(|v| json!(v))
    );
    let log = f.log();
    assert!(
        log.contains("IMAGE_PROD_ORDERS=acme/orders:abc")
            && log.contains("up -d --no-deps prod-orders"),
        "{log}"
    );
    let docs = inspect(&p, instance("a"), &["image/prod-orders"]).await;
    assert_eq!(
        docs["image/prod-orders"],
        json!({"image": "acme/orders:abc"})
    );
}

#[tokio::test]
async fn a_deploy_that_never_comes_up_healthy_is_reported_failed() {
    let f = Fake::new("unhealthy");
    let p = f.live_part();
    let extra = json!({"FAKE_HEALTH": "running unhealthy"});
    p.start(live_start("a", live_config(&f, extra, json!({}))))
        .await
        .unwrap();
    let args =
        json!({"service": "prod-orders", "image": "acme/orders:bad", "sha": "bad", "ref": "7"});
    call(&p, "a", "service_deploy", args).await;
    let ev = advance_until_event(&p, "a", "deployed").await;
    let result = &ev.data["result"];
    assert_eq!(
        (&result["ok"], &result["job_id"]),
        (&json!(false), &json!("7"))
    );
    let error = result["error"].as_str().unwrap();
    assert!(error.contains("unhealthy"), "{error}");
}

#[tokio::test]
async fn a_deploy_to_a_service_the_project_lacks_is_a_tool_error() {
    let f = Fake::new("nosvc");
    let p = f.live_part();
    p.start(live_start("a", live_config(&f, json!({}), json!({}))))
        .await
        .unwrap();
    let args =
        json!({"service": "prod-billing", "image": "acme/billing:1", "sha": "1", "ref": "j"});
    let out = call(&p, "a", "service_deploy", args).await;
    assert_eq!(
        out.error.as_deref(),
        Some("the project has no service prod-billing")
    );
    assert!(!f.log().contains("--no-deps"), "{}", f.log());
}

#[tokio::test]
async fn the_deployed_images_are_saved_and_restored() {
    let f = Fake::new("deploysave");
    let p = f.live_part();
    let config = live_config(&f, json!({}), json!({}));
    p.start(live_start("a", config.clone())).await.unwrap();
    let args =
        json!({"service": "prod-orders", "image": "acme/orders:abc", "sha": "abc", "ref": "j1"});
    call(&p, "a", "service_deploy", args).await;
    advance_until_event(&p, "a", "deployed").await;
    p.save(save_as(&f, "a", "s1")).await.unwrap();
    let manifest = read_json(f.0.join("saves/service/s1/save.json"));
    assert_eq!(manifest["images"]["prod-orders"], "acme/orders:abc");
    let before = f.log().lines().count();
    let mut from = live_start("b", config);
    from.from = Some(FromSave {
        save: "s1".into(),
        mode: Mode::Exact,
    });
    p.start(from).await.unwrap();
    let restored = log_since(&f, before).join("\n");
    assert!(
        restored.contains("IMAGE_PROD_ORDERS=acme/orders:abc"),
        "{restored}"
    );
    assert!(
        !restored.contains("IMAGE_PROD_ORDERS=acme/orders:seed"),
        "{restored}"
    );
    let docs = inspect(&p, instance("b"), &["image/prod-orders"]).await;
    assert_eq!(docs["image/prod-orders"]["image"], "acme/orders:abc");
}

fn probes(port: u16) -> Value {
    json!([
        {"key": "slo/production", "service": "traffic", "port": 9100, "path": "/slo?time={world_s}"},
        {"key": "issues", "service": "glitchtip", "port": 8000, "path": "/issues",
         "headers": {"Authorization": "Bearer t0ken"}},
        {"key": "down", "service": "traffic", "port": 9100, "path": "/down"},
        {"key": "html", "service": "traffic", "port": 9100, "path": "/html"},
        {"key": "at", "service": "traffic", "port": 9100, "path": format!("/at/{{world_ms}}/{port}")},
    ])
}

/// The probed services' answers at world second 1773122405.
fn probed_services() -> Probed {
    let server = Probed::new();
    server.answer("/slo?time=1773122405", 200, r#"{"error_rate_30m": 0.01}"#);
    server.answer("/issues", 200, "[]");
    server.answer("/down", 503, "no");
    server.answer("/html", 200, "<html>");
    let at = format!("/at/1773122405000/{}", server.port);
    server.answer(&at, 200, "1");
    server
}

#[tokio::test]
async fn probes_become_docs_and_are_kept_in_saves() {
    let (f, server) = (Fake::new("probes"), probed_services());
    let port = server.port;
    let p = f.live_part();
    let more = json!({"probes": probes(port)});
    let config = live_config(&f, json!({"FAKE_PORTS": port.to_string()}), more);
    p.start(live_start("a", config)).await.unwrap();
    advance(&p, "a", 5_000).await;
    let keys = ["slo/production", "issues", "down", "html", "at"];
    let docs = inspect(&p, instance("a"), &keys).await;
    assert_eq!(docs["slo/production"], json!({"error_rate_30m": 0.01}));
    assert_eq!((&docs["issues"], &docs["at"]), (&json!([]), &json!(1)));
    for bad in ["down", "html"] {
        assert!(docs[bad]["error"].is_string(), "{bad}: {}", docs[bad]);
    }
    let heads = server.heads.lock().unwrap().clone();
    let issues = heads
        .iter()
        .find(|h| h.starts_with("GET /issues "))
        .unwrap();
    assert!(
        issues
            .to_lowercase()
            .contains("authorization: bearer t0ken"),
        "{issues}"
    );
    p.save(save_as(&f, "a", "s1")).await.unwrap();
    server.answer("/slo?time=1773122405", 200, r#"{"error_rate_30m": 0.5}"#);
    let saved = inspect(&p, Target::Save { save: "s1".into() }, &keys).await;
    assert_eq!(saved["slo/production"], json!({"error_rate_30m": 0.01}));
    assert_eq!(saved["at"], json!(1));
}

fn watch() -> Value {
    json!([{"probe": "alerts", "every_ms": 2000, "items": "/data", "id": "/fp",
            "on": "alert_firing", "off": "alert_resolved",
            "fields": {"summary": "/annotations/summary", "service": "/labels/service"}}])
}

fn alerts(ids: &[&str]) -> String {
    let items: Vec<Value> = (ids.iter())
        .map(|id| json!({"fp": id, "annotations": {"summary": format!("{id} is on")}, "labels": {"service": "orders"}}))
        .collect();
    json!({"data": items}).to_string()
}

/// The (kind, id) of each event an advance to `clock_ms` gives.
async fn watched(p: &Service, clock_ms: i64) -> Vec<(String, String)> {
    let adv = advance(p, "a", clock_ms).await;
    let id = |e: &WorldEvent| e.data["id"].as_str().unwrap_or_default().to_string();
    adv.events.iter().map(|e| (e.kind.clone(), id(e))).collect()
}

#[tokio::test]
async fn a_watch_emits_on_appear_and_disappear() {
    let (f, server) = (Fake::new("watch"), Probed::new());
    let probe =
        json!([{"key": "alerts", "service": "alertmanager", "port": 9093, "path": "/alerts"}]);
    let more = json!({"probes": probe, "watches": watch()});
    let config = live_config(&f, json!({"FAKE_PORTS": server.port.to_string()}), more);
    let p = f.live_part();
    p.start(live_start("a", config)).await.unwrap();
    server.answer("/alerts", 200, &alerts(&["a"]));
    let on = |id: &str| ("alert_firing".to_string(), id.to_string());
    assert_eq!(watched(&p, 2_000).await, vec![on("a")]);
    server.answer("/alerts", 200, &alerts(&["a", "b"]));
    assert_eq!(watched(&p, 3_000).await, vec![], "not due before every_ms");
    assert_eq!(watched(&p, 4_000).await, vec![on("b")]);
    server.answer("/alerts", 503, "down");
    assert_eq!(
        watched(&p, 6_000).await,
        vec![],
        "a failed probe changes nothing"
    );
    server.answer("/alerts", 200, &alerts(&["b"]));
    let adv = advance(&p, "a", 8_000).await;
    assert_eq!(adv.events.len(), 1);
    let gone = &adv.events[0];
    assert_eq!(
        (gone.kind.as_str(), &gone.data["id"]),
        ("alert_resolved", &json!("a"))
    );
    assert_eq!(gone.data["summary"], "a is on");
    assert_eq!(gone.data["service"], "orders");
}

#[tokio::test]
async fn a_restored_watch_knows_what_was_firing_at_its_save() {
    let (f, server) = (Fake::new("watchsave"), Probed::new());
    let probe =
        json!([{"key": "alerts", "service": "alertmanager", "port": 9093, "path": "/alerts"}]);
    let more = json!({"probes": probe, "watches": watch()});
    let config = live_config(&f, json!({"FAKE_PORTS": server.port.to_string()}), more);
    let p = f.live_part();
    p.start(live_start("a", config.clone())).await.unwrap();
    server.answer("/alerts", 200, &alerts(&["a"]));
    assert_eq!(watched(&p, 2_000).await.len(), 1);
    p.save(save_as(&f, "a", "s1")).await.unwrap();
    let mut from = live_start("b", config);
    from.clock_ms = 2_000;
    from.from = Some(FromSave {
        save: "s1".into(),
        mode: Mode::Exact,
    });
    p.start(from).await.unwrap();
    let adv = advance(&p, "b", 4_000).await;
    assert!(adv.events.is_empty(), "{:?}", adv.events);
}

#[tokio::test]
async fn a_live_copy_volume_stops_nothing() {
    let f = Fake::new("livecopy");
    let p = f.live_part();
    let volumes = json!([{"name": "prom", "save": "hardlink", "live_copy": true},
                         {"name": "logs", "live_copy": true}]);
    let mut config = live_config(&f, json!({}), json!({}));
    config["volumes"] = volumes;
    p.start(live_start("a", config)).await.unwrap();
    let before = f.log().lines().count();
    p.save(save_as(&f, "a", "s1")).await.unwrap();
    let lines = log_since(&f, before);
    assert!(
        !lines
            .iter()
            .any(|l| l.contains(" stop") || l.contains(" start")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("arena_a_prom:/v:ro")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("arena_a_logs:/v")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn a_live_start_waits_for_health_and_lets_one_shots_finish() {
    let f = Fake::new("liveup");
    let extra = json!({"FAKE_PS": "web\trunning\thealthy\t0\nseed\texited\t\t0"});
    f.live_part()
        .start(live_start("a", live_config(&f, extra, json!({}))))
        .await
        .unwrap();
    let log = f.log();
    assert!(log.lines().any(|l| l.ends_with(" up -d")), "{log}");
    assert!(!log.contains("--wait"), "{log}");
    assert!(log.contains(" ps -a --format "), "{log}");
}

#[tokio::test]
async fn a_live_start_fails_when_a_one_shot_fails() {
    let f = Fake::new("liveshot");
    let extra = json!({"FAKE_PS": "web\trunning\thealthy\t0\nseed\texited\t\t3"});
    let e = f
        .live_part()
        .start(live_start("a", live_config(&f, extra, json!({}))))
        .await
        .unwrap_err();
    assert!(e.message.contains("seed exited with 3"), "{}", e.message);
    assert!(f.log().trim_end().ends_with(" down -v"), "{}", f.log());
}
