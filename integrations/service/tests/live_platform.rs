//! The software factory's real platform (template `software-factory`, `platform/`) run by the
//! live-service part under the lean profile: `cargo test -p arena-service --test live_platform --
//! --ignored live_`. Needs Docker, the images of the template's `images.yaml` (pulled, and built by
//! `images/build.sh` and `tools/worldclock-gateway/build-image.sh`) and `python3` with PyYAML to
//! read the template's YAML. It deploys a new orders image (a new tag of the seeded one), checks
//! `/version` and the `deployed` event, reads the services' telemetry back through the world-time
//! gateway in world time (the world runs 10 minutes behind the wall), saves, and starts a second
//! platform from that save. Every project, image tag and folder it makes is removed afterwards.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use arena_part::helpers::http::reqwest;
use arena_part::proto::*;
use arena_part::Part;
use arena_service::Service;
use serde_json::{json, Value};

const DOMAIN: &str = "shop.test";
/// How far the world runs behind the wall.
const BEHIND_MS: i64 = 600_000;

fn template() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.join("../../benchmarks/world/templates/software-factory")
}

/// A YAML file of the template as JSON (by python3's PyYAML: no YAML reader in this workspace).
fn yaml(path: &Path) -> Value {
    let code = "import json, sys, yaml; print(json.dumps(yaml.safe_load(open(sys.argv[1]))))";
    let out = Command::new("python3")
        .args(["-c", code])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// The platform's service config as the template gives it, under the lean profile, its hosts
/// in `DOMAIN`.
fn platform_config() -> Value {
    let platform = template().join("platform");
    let text = yaml(&platform.join("service.yaml")).to_string();
    let mut c: Value = serde_json::from_str(&text.replace("{company.domain}", DOMAIN)).unwrap();
    c["compose_file"] = json!(platform.join("compose.yaml"));
    let lean = yaml(&platform.join("profiles/lean.yaml"));
    let env = c["env"].as_object_mut().unwrap();
    for (k, v) in lean.as_object().unwrap() {
        let v = v.as_str().map_or_else(|| v.to_string(), str::to_string);
        env.insert(k.clone(), json!(v));
    }
    c
}

/// The instances and image tags the test made; dropping it removes them and the folder.
struct Made {
    dir: PathBuf,
    projects: Vec<String>,
    tags: Vec<String>,
}

impl Drop for Made {
    fn drop(&mut self) {
        for p in &self.projects {
            let down = ["compose", "-p", p, "down", "-v", "--remove-orphans"];
            let _ = Command::new("docker")
                .args(down)
                .env_remove("COMPOSE_PROJECT_NAME")
                .output();
        }
        for t in &self.tags {
            let _ = Command::new("docker").args(["rmi", t]).output();
        }
        let _ = Command::new("docker")
            .args(["run", "--rm", "-v"])
            .arg(format!("{}:/d", self.dir.display()))
            .args(["redis:6.2-alpine", "rm", "-rf", "/d/saves"])
            .output();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn wall_ms() -> i64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.unwrap().as_millis() as i64
}

fn start_params(id: &str, from: Option<&str>, clock_ms: i64, epoch_ms: i64) -> StartParams {
    StartParams {
        instance: id.into(),
        config: platform_config(),
        seed: 1,
        clock_ms,
        from: from.map(|s| FromSave {
            save: s.into(),
            mode: Mode::Exact,
        }),
        world_time: Some(WorldTimeParams {
            epoch_ms,
            speed: 1.0,
        }),
    }
}

/// The instance's clock now that the world started at `epoch_ms`.
fn clock_now(epoch_ms: i64) -> i64 {
    wall_ms() - BEHIND_MS - epoch_ms
}

async fn advance(p: &Service, id: &str, clock_ms: i64) -> Advanced {
    let at = AdvanceParams {
        instance: id.into(),
        clock_ms,
    };
    p.advance(at).await.unwrap()
}

async fn doc(p: &Service, id: &str, key: &str) -> Value {
    let target = Target::Instance {
        instance: id.into(),
    };
    let docs = p
        .inspect(InspectParams {
            target,
            keys: vec![key.into()],
        })
        .await
        .unwrap();
    docs.docs[key].clone()
}

/// The published port of `service`'s `port` in project `arena_<id>`.
fn published(id: &str, service: &str, port: u16) -> String {
    let args = [
        "compose",
        "-p",
        &format!("arena_{id}"),
        "port",
        service,
        &port.to_string(),
    ];
    let out = Command::new("docker").args(args).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let port = text.trim().rsplit_once(':').unwrap().1.to_string();
    format!("http://127.0.0.1:{port}")
}

/// Advances `id` each second (the world clock running) until `done` gives a value, at most `within`.
async fn until<T>(
    p: &Service,
    id: &str,
    epoch_ms: i64,
    within: Duration,
    mut done: impl AsyncFnMut(&Advanced) -> Option<T>,
) -> T {
    let since = Instant::now();
    loop {
        let adv = advance(p, id, clock_now(epoch_ms)).await;
        if let Some(v) = done(&adv).await {
            return v;
        }
        assert!(since.elapsed() < within, "nothing within {within:?}");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// A new tag of the seeded orders image (what a pipeline's build would push for a new commit).
fn new_orders_image(made: &mut Made, config: &Value) -> (String, String) {
    let seeded = config["images"]["prod-orders"].as_str().unwrap();
    let sha = format!("live{}", std::process::id());
    let image = format!("{}:{sha}", seeded.rsplit_once(':').unwrap().0);
    let ok = Command::new("docker")
        .args(["tag", seeded, &image])
        .status()
        .unwrap();
    assert!(ok.success(), "tagging {seeded}");
    made.tags.push(image.clone());
    (image, sha)
}

/// The JSON of a GET through the gateway's query face; None (said why) when it isn't 2xx.
async fn ask(url: String) -> Option<Value> {
    let answer = reqwest::get(&url).await.ok()?;
    let status = answer.status();
    let text = answer.text().await.unwrap_or_default();
    if !status.is_success() {
        eprintln!("{url}: {status} {text}");
        return None;
    }
    serde_json::from_str(&text).ok()
}

/// The newest stamp (ms) in the first matrix/vector/streams/traces answer that has one.
async fn newest_stamps(gateway: &str) -> Option<(i64, i64, i64)> {
    let metric = ask(format!(
        "{gateway}/q/prometheus/api/v1/query?query=max(timestamp(deploy_info))"
    ))
    .await?;
    let m = metric["data"]["result"][0]["value"][1]
        .as_str()?
        .parse::<f64>()
        .ok()?
        * 1000.0;
    let logs = ask(format!(
        "{gateway}/q/loki/loki/api/v1/query_range?query=%7Bservice_name%3D~%22.%2B%22%7D&limit=5"
    ))
    .await?;
    let values = logs["data"]["result"]
        .as_array()?
        .iter()
        .flat_map(|s| s["values"].as_array().cloned().unwrap_or_default());
    let l = values
        .filter_map(|v| v[0].as_str()?.parse::<i64>().ok())
        .max()?
        / 1_000_000;
    let traces = ask(format!("{gateway}/q/tempo/api/search?limit=20")).await?;
    let t = (traces["traces"].as_array()?.iter())
        .filter_map(|t| t["startTimeUnixNano"].as_str()?.parse::<i64>().ok())
        .max()?
        / 1_000_000;
    Some((m as i64, l, t))
}

/// Deploys a new orders image to `id`: it comes up healthy, `deployed` says so and `/version` has its sha.
async fn deploys_a_new_orders_image(
    p: &Service,
    id: &str,
    epoch: i64,
    made: &mut Made,
) -> (String, String) {
    let (image, sha) = new_orders_image(made, &platform_config());
    let args = json!({"service": "prod-orders", "image": image, "sha": sha, "ref": "j1"});
    let call = CallToolParams {
        instance: id.into(),
        name: "service_deploy".into(),
        args,
        clock_ms: clock_now(epoch),
    };
    assert_eq!(
        p.call_tool(call).await.unwrap().result,
        Some(json!({"accepted": true}))
    );
    let ev = until(
        p,
        id,
        epoch,
        Duration::from_secs(400),
        async |adv: &Advanced| adv.events.iter().find(|e| e.kind == "deployed").cloned(),
    )
    .await;
    assert_eq!(ev.data["result"]["ok"], true, "{ev:?}");
    assert_eq!(ev.data["result"]["job_id"], "j1");
    assert_eq!(doc(p, id, "version/orders").await["sha"], json!(sha));
    (image, sha)
}

/// The shop's metrics, logs and traces, read through the gateway, are stamped in world time.
async fn telemetry_is_in_world_time(p: &Service, id: &str, epoch: i64) {
    let storefront = published(id, "prod-storefront", 8000);
    let gateway = published(id, "worldclock-gateway", 4319);
    let stamps = until(
        p,
        id,
        epoch,
        Duration::from_secs(180),
        async |_: &Advanced| {
            for path in ["/products", "/healthz"] {
                let _ = reqwest::get(format!("{storefront}{path}")).await;
            }
            newest_stamps(&gateway).await
        },
    )
    .await;
    let world_now = wall_ms() - BEHIND_MS;
    for (what, t) in [("metric", stamps.0), ("log", stamps.1), ("trace", stamps.2)] {
        let recent = world_now - 180_000..=world_now + 5_000;
        assert!(recent.contains(&t), "{what} at {t}, world now {world_now}");
    }
}

/// A save of `id` keeps the deployed image, and a platform `id2` started from it runs it.
async fn the_save_runs_the_deployed_image(
    p: &Service,
    ids: (&str, &str),
    epoch: i64,
    dir: &Path,
    image_sha: (&str, &str),
) {
    let save_dir = dir.join("saves/live-service/s1");
    let save = SaveParams {
        instance: ids.0.into(),
        save: "s1".into(),
        dir: save_dir.to_string_lossy().into(),
    };
    p.save(save).await.unwrap();
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(save_dir.join("save.json")).unwrap()).unwrap();
    assert_eq!(manifest["images"]["prod-orders"], json!(image_sha.0));
    p.stop(IdParams {
        instance: ids.0.into(),
    })
    .await
    .unwrap();
    p.start(start_params(ids.1, Some("s1"), clock_now(epoch), epoch))
        .await
        .unwrap();
    assert_eq!(
        doc(p, ids.1, "version/orders").await["sha"],
        json!(image_sha.1)
    );
    p.stop(IdParams {
        instance: ids.1.into(),
    })
    .await
    .unwrap();
}

#[tokio::test]
#[ignore]
async fn live_platform_deploys_a_new_image() {
    let dir = std::env::temp_dir().join(format!("arena_live_platform_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (id, id2) = (
        format!("lp{}", std::process::id()),
        format!("lq{}", std::process::id()),
    );
    let projects = vec![format!("arena_{id}"), format!("arena_{id2}")];
    let mut made = Made {
        dir: dir.clone(),
        projects,
        tags: vec![],
    };
    let p = Service::live(dir.join("saves/live-service"));
    let epoch = wall_ms() - BEHIND_MS;
    p.start(start_params(&id, None, 0, epoch)).await.unwrap();
    assert!(doc(&p, &id, "version/orders").await["sha"].is_string());
    let (image, sha) = deploys_a_new_orders_image(&p, &id, epoch, &mut made).await;
    telemetry_is_in_world_time(&p, &id, epoch).await;
    the_save_runs_the_deployed_image(&p, (&id, &id2), epoch, &dir, (&image, &sha)).await;
    drop(made);
}
