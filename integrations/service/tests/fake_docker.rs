//! Service behaviour when Docker misbehaves, with a fake `docker` program first on the PATH.
use arena_part::proto::*;
use arena_part::Part;
use arena_service::Service;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// The fake: logs its arguments, and fails where the environment says so. `port` answers the
/// n-th of `FAKE_PORTS` after the n-th `up` or `start` (a restarted service gets a new port).
const FAKE: &str = r#"#!/bin/sh
echo "$@" >> "$FAKE_LOG"
case "$*" in
  *" stop") [ -n "$FAKE_STOP_FAIL" ] && { echo no stop >&2; exit 1; } ;;
  *" up "*) [ -n "$FAKE_UP_FAIL" ] && { echo no up >&2; exit 1; } ;;
  *-cpf*) case "$*" in *_bad:/v*) echo no export >&2; exit 1 ;; *) printf tar ;; esac ;;
  *" port "*) n=$(grep -cE ' (start|up)( |$)' "$FAKE_LOG")
             echo "127.0.0.1:$(echo "${FAKE_PORTS:-49153}" | awk -v n="$n" '{print $(n < NF ? n : NF)}')" ;;
  *pg_dump*) printf -- '-- dumped at some time\nCREATE TABLE t (n int);\n' ;;
esac
exit 0
"#;

/// A temporary folder with the fake docker; removed on drop.
struct Fake(PathBuf);

impl Fake {
    fn new(tag: &str) -> Fake {
        let dir = std::env::temp_dir().join(format!("arena_fake_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bin")).unwrap();
        let docker = dir.join("bin/docker");
        fs::write(&docker, FAKE).unwrap();
        fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(dir.join("compose.yml"), "services: {}\n").unwrap();
        Fake(dir)
    }

    fn part(&self) -> Service {
        Service::new(self.0.join("saves/service"))
    }

    /// A config whose compose runs go to the fake; `extra` adds environment variables.
    fn config(&self, volumes: &[&str], extra: Value) -> Value {
        let path = format!(
            "{}:{}",
            self.0.join("bin").display(),
            std::env::var("PATH").unwrap()
        );
        let mut env = json!({"PATH": path, "FAKE_LOG": self.log_path()});
        env.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json!({"compose_file": self.0.join("compose.yml"), "volumes": volumes, "env": env})
    }

    fn log_path(&self) -> String {
        self.0.join("log").to_string_lossy().into()
    }

    fn log(&self) -> String {
        fs::read_to_string(self.0.join("log")).unwrap_or_default()
    }

    fn live_file(&self, id: &str) -> PathBuf {
        self.0.join(format!("saves/service/.live/{id}.json"))
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn start(id: &str, config: Value) -> StartParams {
    StartParams {
        instance: id.into(),
        config,
        seed: 1,
        clock_ms: 0,
        from: None,
        world_time: None,
    }
}

fn save_params(f: &Fake, id: &str) -> SaveParams {
    let dir = f.0.join("saves/service/s1");
    SaveParams {
        instance: id.into(),
        save: "s1".into(),
        dir: dir.to_string_lossy().into(),
    }
}

#[tokio::test]
async fn failed_stop_still_starts_and_leaves_no_save() {
    let f = Fake::new("stop");
    let p = f.part();
    p.start(start(
        "a",
        f.config(&["data"], json!({"FAKE_STOP_FAIL": "1"})),
    ))
    .await
    .unwrap();
    let e = p.save(save_params(&f, "a")).await.unwrap_err();
    assert!(e.message.contains("no stop"), "{e}");
    assert!(f.log().trim_end().ends_with(" start"), "{}", f.log());
    assert!(!f.0.join("saves/service/s1").exists());
}

#[tokio::test]
async fn failed_export_leaves_no_partial_save() {
    let f = Fake::new("export");
    let p = f.part();
    p.start(start("a", f.config(&["data", "bad"], json!({}))))
        .await
        .unwrap();
    let e = p.save(save_params(&f, "a")).await.unwrap_err();
    assert!(e.message.contains("no export"), "{e}");
    assert!(
        f.log().trim_end().ends_with(" start"),
        "the project is started again"
    );
    assert!(!f.0.join("saves/service/s1").exists());
}

#[tokio::test]
async fn good_save_writes_archives() {
    let f = Fake::new("good");
    let p = f.part();
    p.start(start("a", f.config(&["data"], json!({}))))
        .await
        .unwrap();
    let saved = p.save(save_params(&f, "a")).await.unwrap();
    assert_eq!(saved.bytes, 3);
    assert!(f.0.join("saves/service/s1/data.tar").is_file());
}

#[tokio::test]
async fn failed_up_forgets_the_instance() {
    let f = Fake::new("up");
    let p = f.part();
    let e = p
        .start(start(
            "a",
            f.config(&["data"], json!({"FAKE_UP_FAIL": "1"})),
        ))
        .await
        .unwrap_err();
    assert!(e.message.contains("no up"), "{e}");
    assert!(!f.live_file("a").exists());
    assert!(f.log().contains(" down -v"), "a failed start is cleaned up");
}

#[tokio::test]
async fn failed_record_brings_nothing_up() {
    let f = Fake::new("record");
    fs::create_dir_all(f.0.join("saves/service")).unwrap();
    fs::write(f.0.join("saves/service/.live"), "in the way").unwrap();
    let e = f
        .part()
        .start(start("a", f.config(&["data"], json!({}))))
        .await
        .unwrap_err();
    assert_eq!(e.code, -32603);
    assert!(f.log().is_empty(), "docker ran: {}", f.log());
}

#[tokio::test]
async fn bad_helper_image_is_invalid_params() {
    let f = Fake::new("image");
    for image in ["", "-v", "a b", "x;y"] {
        let mut config = f.config(&["data"], json!({}));
        config["helper_image"] = json!(image);
        let e = f.part().start(start("a", config)).await.unwrap_err();
        assert_eq!(e.code, -32602, "{image:?}");
    }
    assert!(f.log().is_empty());
}

#[tokio::test]
async fn a_restarted_program_still_saves_inspects_and_stops_a_running_instance() {
    let f = Fake::new("restart");
    let config = f.config(&["data"], json!({}));
    f.part().start(start("a", config.clone())).await.unwrap();
    let record = fs::read_to_string(f.live_file("a")).unwrap();
    assert_eq!(record, config.to_string(), "the record is the config");
    let again = f.part(); // the program restarted: nothing in memory, the record on disk
    assert_eq!(again.save(save_params(&f, "a")).await.unwrap().bytes, 3);
    let target = Target::Instance {
        instance: "a".into(),
    };
    let docs = again.inspect(InspectParams {
        target,
        keys: vec![],
    });
    assert!(docs.await.unwrap().docs.contains_key("volume/data"));
    again
        .stop(IdParams {
            instance: "a".into(),
        })
        .await
        .unwrap();
    assert!(!f.live_file("a").exists());
    assert!(f.log().contains(" down -v"), "{}", f.log());
    let gone = again.save(save_params(&f, "a")).await.unwrap_err();
    assert_eq!(gone.message, "no instance a");
}

/// A config with `volumes` given as JSON (names or specs) and `extra` top-level fields.
fn config_with(f: &Fake, volumes: Value, extra: Value) -> Value {
    let mut c = f.config(&[], json!({}));
    c["volumes"] = volumes;
    c.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    c
}

fn save_as(f: &Fake, id: &str, save: &str) -> SaveParams {
    let dir = f.0.join("saves/service").join(save);
    SaveParams {
        instance: id.into(),
        save: save.into(),
        dir: dir.to_string_lossy().into(),
    }
}

/// The log lines written since `from` lines.
fn log_since(f: &Fake, from: usize) -> Vec<String> {
    f.log().lines().skip(from).map(str::to_string).collect()
}

const RSYNC: &str =
    "example/rsync:3.2@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn plain_volume_names_save_as_before() {
    let f = Fake::new("plainnames");
    let p = f.part();
    p.start(start("a", f.config(&["data"], json!({}))))
        .await
        .unwrap();
    let before = f.log().lines().count();
    p.save(save_params(&f, "a")).await.unwrap();
    let lines = log_since(&f, before);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(
        lines[0].ends_with(" stop") && lines[2].ends_with(" start"),
        "{lines:?}"
    );
    let export = "--network none --entrypoint tar -v arena_a_data:/v redis:6.2-alpine -C /v -cpf - --numeric-owner .";
    assert!(
        lines[1].starts_with("run --rm --pull never ") && lines[1].ends_with(export),
        "{lines:?}"
    );
}

#[tokio::test]
async fn a_postgres_volume_is_dumped_without_stopping_anything() {
    let f = Fake::new("pgdump");
    let p = f.part();
    let volumes = json!([{"name": "pg", "save": "postgres", "service": "db"}]);
    p.start(start("a", config_with(&f, volumes, json!({}))))
        .await
        .unwrap();
    p.save(save_params(&f, "a")).await.unwrap();
    let log = f.log();
    assert!(
        log.contains("exec -T db pg_dump -U postgres --no-owner --no-privileges postgres"),
        "{log}"
    );
    assert!(
        !log.lines()
            .any(|l| l.ends_with(" stop") || l.contains(" stop ")),
        "{log}"
    );
    let dir = f.0.join("saves/service/s1");
    assert!(dir.join("pg.sql").is_file() && dir.join("save.json").is_file());
}

#[tokio::test]
async fn a_postgres_save_is_restored_with_psql_before_the_rest_starts() {
    let f = Fake::new("pgrestore");
    let p = f.part();
    let volumes = json!([{"name": "pg", "save": "postgres", "service": "db", "database": "app", "user": "u"}]);
    let config = config_with(&f, volumes, json!({}));
    p.start(start("a", config.clone())).await.unwrap();
    p.save(save_params(&f, "a")).await.unwrap();
    let before = f.log().lines().count();
    let mut from = start("b", config);
    from.from = Some(FromSave {
        save: "s1".into(),
        mode: Mode::Exact,
    });
    p.start(from).await.unwrap();
    let lines = log_since(&f, before);
    let at = |needle: &str| {
        lines
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle}: {lines:?}"))
    };
    assert!(at(" up -d db") < at("exec -T db pg_isready -h 127.0.0.1 -U u"));
    assert!(at("pg_isready") < at("exec -T db psql -h 127.0.0.1 -U u -d app"));
    assert!(at("exec -T db psql") < lines.iter().rposition(|l| l.ends_with(" up -d")).unwrap());
}

#[tokio::test]
async fn a_skipped_volume_is_neither_saved_nor_restored() {
    let f = Fake::new("skipvol");
    let p = f.part();
    let volumes = json!(["data", {"name": "cache", "save": "skip"}]);
    let config = config_with(&f, volumes, json!({}));
    p.start(start("a", config.clone())).await.unwrap();
    p.save(save_params(&f, "a")).await.unwrap();
    let mut from = start("b", config);
    from.from = Some(FromSave {
        save: "s1".into(),
        mode: Mode::Exact,
    });
    p.start(from).await.unwrap();
    let log = f.log();
    assert!(
        log.contains("arena_a_data:/v") && log.contains("arena_b_data:/v"),
        "{log}"
    );
    assert!(!log.contains("_cache:/v"), "{log}");
}

/// The logged `rsync` runs.
fn rsync_lines(f: &Fake) -> Vec<String> {
    let lines = f.log().lines().map(str::to_string).collect::<Vec<_>>();
    lines
        .into_iter()
        .filter(|l| l.contains(" rsync "))
        .collect()
}

#[tokio::test]
async fn a_hardlink_volume_links_against_the_previous_save() {
    let f = Fake::new("hardlink");
    let p = f.part();
    let volumes = json!([{"name": "files", "save": "hardlink"}]);
    p.start(start(
        "a",
        config_with(&f, volumes, json!({"rsync_image": RSYNC})),
    ))
    .await
    .unwrap();
    p.save(save_as(&f, "a", "s1")).await.unwrap();
    p.save(save_as(&f, "a", "s2")).await.unwrap();
    let rsyncs = rsync_lines(&f);
    assert_eq!(rsyncs.len(), 2, "{rsyncs:?}");
    assert!(
        rsyncs
            .iter()
            .all(|l| l.contains("--pull missing") && l.contains(RSYNC)),
        "{rsyncs:?}"
    );
    assert!(!rsyncs[0].contains("--link-dest"), "{}", rsyncs[0]);
    assert!(
        rsyncs[1].contains("--link-dest=/saves/s1/files/"),
        "{}",
        rsyncs[1]
    );
    assert!(rsyncs[1].ends_with("/v/ /saves/s2/files/"), "{}", rsyncs[1]);
}

#[tokio::test]
async fn file_volumes_that_name_their_services_stop_only_those() {
    let f = Fake::new("partstop");
    let p = f.part();
    let volumes = json!([{"name": "prom", "services": ["prom"]},
                         {"name": "logs", "save": "hardlink", "services": ["loki"]}]);
    p.start(start(
        "a",
        config_with(&f, volumes, json!({"rsync_image": RSYNC})),
    ))
    .await
    .unwrap();
    p.save(save_params(&f, "a")).await.unwrap();
    let log = f.log();
    assert!(log.lines().any(|l| l.ends_with(" stop prom loki")), "{log}");
    assert!(
        log.lines().any(|l| l.ends_with(" start prom loki")),
        "{log}"
    );
    assert!(!log.lines().any(|l| l.ends_with(" stop")), "{log}");
}

/// A port something listens on (while the listener lives).
fn listening() -> (std::net::TcpListener, u16) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    (l, port)
}

fn routes_of(id: &str) -> IdParams {
    IdParams {
        instance: id.into(),
    }
}

#[tokio::test]
async fn the_service_serves_the_hosts_its_config_names() {
    let f = Fake::new("routes");
    let (_l, port) = listening();
    let p = f.part();
    let http = json!([{"host": "app.acme.test", "service": "web", "port": 8080,
                       "env": {"APP_URL": "https://app.acme.test"}}]);
    let mut config = f.config(&["data"], json!({"FAKE_PORTS": port.to_string()}));
    config["http"] = http;
    p.start(start("a", config)).await.unwrap();
    let routes = p.http_routes(routes_of("a")).await.unwrap();
    assert_eq!(routes.routes.len(), 1);
    assert_eq!(routes.routes[0].host, "app.acme.test");
    assert_eq!(
        routes.routes[0].upstream,
        format!("http://127.0.0.1:{port}")
    );
    assert_eq!(routes.routes[0].env["APP_URL"], "https://app.acme.test");
    assert!(f.log().contains(" port web 8080"), "{}", f.log());
    assert_eq!(p.describe().await.unwrap().save_at, SaveAt::Points);
}

#[tokio::test]
async fn after_a_save_restarts_it_the_service_is_served_at_its_new_port_once_it_listens() {
    let f = Fake::new("newport");
    let (_first, first) = listening();
    let second = listening().1; // nothing listens there until the restarted service does
    let p = f.part();
    let mut config = f.config(
        &["data"],
        json!({"FAKE_PORTS": format!("{first} {second}")}),
    );
    config["http"] = json!([{"host": "app.acme.test", "service": "web", "port": 8080}]);
    p.start(start("a", config)).await.unwrap();
    let upstream = |r: HttpRoutes| r.routes[0].upstream.clone();
    let before = upstream(p.http_routes(routes_of("a")).await.unwrap());
    assert_eq!(before, format!("http://127.0.0.1:{first}"));
    p.save(save_params(&f, "a")).await.unwrap();
    let late = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        let l = std::net::TcpListener::bind(("127.0.0.1", second)).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        drop(l);
    });
    let after = upstream(p.http_routes(routes_of("a")).await.unwrap());
    assert_eq!(after, format!("http://127.0.0.1:{second}"));
    let up = std::net::TcpStream::connect(("127.0.0.1", second));
    assert!(
        up.is_ok(),
        "the routes came back before the service listened"
    );
    late.join().unwrap();
}

#[tokio::test]
async fn a_hardlink_save_is_dropped_through_the_helper() {
    let f = Fake::new("droplink");
    let p = f.part();
    let volumes = json!([{"name": "files", "save": "hardlink"}]);
    p.start(start(
        "a",
        config_with(&f, volumes, json!({"rsync_image": RSYNC})),
    ))
    .await
    .unwrap();
    p.save(save_as(&f, "a", "s1")).await.unwrap();
    p.drop_save(SaveId { save: "s1".into() }).await.unwrap();
    assert!(f.log().contains("rm -rf /saves/s1"), "{}", f.log());
    assert!(!f.0.join("saves/service/s1").exists());
}
