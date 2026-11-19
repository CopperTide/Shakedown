//! Runs real Docker Compose projects: `cargo test -p arena-service -- --ignored live_`.
//! Every project, volume and folder the test makes is removed afterwards, even on failure.
use arena_part::proto::*;
use arena_part::Part;
use arena_service::Service;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const COMPOSE: &str = "services:\n  redis:\n    image: redis:6.2-alpine\n    entrypoint: [redis-server]\n    volumes:\n      - data:/data\nvolumes:\n  data: {}\n";

/// The test's folder and compose projects; dropping it removes them all.
struct World {
    dir: PathBuf,
    projects: Vec<String>,
}

impl World {
    fn new() -> World {
        let dir = std::env::temp_dir().join(format!("arena_live_service_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("compose.yml"), COMPOSE).unwrap();
        World {
            dir,
            projects: vec![],
        }
    }

    fn part(&self) -> Service {
        Service::new(self.dir.join("saves/service"))
    }

    fn config(&self) -> Value {
        json!({"compose_file": self.dir.join("compose.yml"), "volumes": ["data"]})
    }

    /// Runs `args` in the instance's container and returns its trimmed output.
    fn exec(&self, instance: &str, args: &[&str]) -> String {
        let out = Command::new("docker")
            .args(["exec", &format!("arena_{instance}-redis-1")])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn redis(&self, instance: &str, args: &[&str]) -> String {
        let cli = [&["redis-cli"], args].concat();
        self.exec(instance, &cli)
    }

    /// Starts instance `id` (tracked for removal) from `from`.
    async fn start(&mut self, p: &Service, id: &str, from: Option<&str>) {
        self.projects.push(format!("arena_{id}"));
        p.start(start(id, self.config(), from)).await.unwrap();
    }
}

impl Drop for World {
    fn drop(&mut self) {
        for p in &self.projects {
            let _ = Command::new("docker")
                .args(["compose", "-p", p, "down", "-v"])
                .env_remove("COMPOSE_PROJECT_NAME")
                .output();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn start(instance: &str, config: Value, from: Option<&str>) -> StartParams {
    StartParams {
        instance: instance.into(),
        config,
        seed: 1,
        clock_ms: 0,
        from: from.map(|s| FromSave {
            save: s.into(),
            mode: Mode::Exact,
        }),
        world_time: None,
    }
}

fn instance(i: &str) -> Target {
    Target::Instance { instance: i.into() }
}

fn save(s: &str) -> Target {
    Target::Save { save: s.into() }
}

async fn differing(p: &Service, a: Target, b: Target) -> Vec<String> {
    p.diff(DiffParams { a, b }).await.unwrap().differing
}

async fn save_to(w: &World, p: &Service, instance: &str, id: &str) -> PathBuf {
    let dir = w.dir.join("saves/service").join(id);
    let params = SaveParams {
        instance: instance.into(),
        save: id.into(),
        dir: dir.to_string_lossy().into(),
    };
    let saved = p.save(params).await.unwrap();
    assert!(saved.exact && saved.bytes > 0, "{saved:?}");
    dir
}

/// Sets a key and a file with an unusual owner in instance `a`, then saves it as `s1`.
async fn first_save(w: &World, a: &str) -> PathBuf {
    assert_eq!(w.redis(a, &["set", "k", "before"]), "OK");
    w.exec(
        a,
        &["sh", "-c", "echo x > /data/own; chown 1234:4321 /data/own"],
    );
    // a program restarted since the instance began still saves it
    save_to(w, &w.part(), a, "s1").await
}

/// Changes `a` after the save: the change stays out of the save.
async fn diverge(w: &World, p: &Service, a: &str) {
    // the project runs again after the save
    assert_eq!(w.redis(a, &["get", "k"]), "before");
    assert_eq!(w.redis(a, &["set", "k", "after"]), "OK");
    assert_eq!(w.redis(a, &["save"]), "OK"); // a live diff sees what is on disk
    assert_eq!(differing(p, instance(a), save("s1")).await, ["volume/data"]);
}

/// The instance `b`, started from `s1`, has the saved key, the saved owner and the saved content.
async fn check_restored(w: &World, p: &Service, b: &str) {
    assert_eq!(w.redis(b, &["get", "k"]), "before");
    assert_eq!(
        w.exec(b, &["stat", "-c", "%u:%g", "/data/own"]),
        "1234:4321"
    );
    let same = differing(p, instance(b), save("s1")).await;
    assert!(same.is_empty(), "{same:?}");
    let keys = vec!["volume/data".into(), "volume/nope".into()];
    let docs = p
        .inspect(InspectParams {
            target: save("s1"),
            keys,
        })
        .await
        .unwrap()
        .docs;
    assert!(docs["volume/data"]["files"].as_u64().unwrap() >= 2);
    assert_eq!(docs["volume/nope"], Value::Null);
}

#[tokio::test]
#[ignore]
async fn live_service_roundtrip() {
    let id = std::process::id();
    let (a, b) = (format!("t10_{id}_a"), format!("t10_{id}_b"));
    let mut w = World::new();
    let p = w.part();
    w.start(&p, &a, None).await;
    let dir = first_save(&w, &a).await;
    diverge(&w, &p, &a).await;
    w.start(&p, &b, Some("s1")).await;
    check_restored(&w, &p, &b).await;
    assert_eq!(w.redis(&a, &["get", "k"]), "after");
    for i in [&a, &b] {
        w.part()
            .stop(IdParams {
                instance: i.clone(),
            })
            .await
            .unwrap();
    }
    p.drop_save(SaveId { save: "s1".into() }).await.unwrap();
    assert!(!dir.exists());
}

/// A project with a Postgres database (`postgres` volume `pg`), a file holder (`hardlink`
/// volume `files`) and a cache (`skip`): images from `ARENA_TEST_POSTGRES_IMAGE` (default
/// `postgres:17-alpine`) and `ARENA_TEST_RSYNC_IMAGE` (an image with rsync, pinned by digest).
fn strategies_compose(pg_image: &str) -> String {
    format!(
        "services:\n  db:\n    image: {pg_image}\n    environment: {{POSTGRES_PASSWORD: pw}}\n    volumes: [pg:/var/lib/postgresql/data]\n  \
         files:\n    image: redis:6.2-alpine\n    entrypoint: [sleep, infinity]\n    volumes: [files:/files, cache:/cache]\n\
         volumes:\n  pg: {{}}\n  files: {{}}\n  cache: {{}}\n"
    )
}

fn sql(w: &World, instance: &str, query: &str) -> String {
    let out = Command::new("docker")
        .args([
            "exec",
            &format!("arena_{instance}-db-1"),
            "psql",
            "-U",
            "postgres",
            "-Atc",
            query,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = w;
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn files(instance: &str, script: &str) -> String {
    let out = Command::new("docker")
        .args([
            "exec",
            &format!("arena_{instance}-files-1"),
            "sh",
            "-c",
            script,
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The strategies' world and config, or None without `ARENA_TEST_RSYNC_IMAGE`.
fn strategies_world() -> Option<(World, Value)> {
    let Ok(rsync) = std::env::var("ARENA_TEST_RSYNC_IMAGE") else {
        eprintln!("live_service_strategies needs ARENA_TEST_RSYNC_IMAGE");
        return None;
    };
    let pg = std::env::var("ARENA_TEST_POSTGRES_IMAGE").unwrap_or("postgres:17-alpine".into());
    let w = World::new();
    fs::write(w.dir.join("compose.yml"), strategies_compose(&pg)).unwrap();
    let volumes = json!([{"name": "pg", "save": "postgres", "service": "db"},
        {"name": "files", "save": "hardlink", "services": ["files"]},
        {"name": "cache", "save": "skip"}]);
    let file = w.dir.join("compose.yml");
    Some((
        w,
        json!({"compose_file": file, "rsync_image": rsync, "volumes": volumes}),
    ))
}

/// Saves `a` twice (a row and a file changed between): the two saves' paths.
async fn two_saves(w: &World, p: &Service, a: &str) -> (PathBuf, PathBuf) {
    sql(w, a, "create table t (n int); insert into t values (1)");
    files(
        a,
        "echo one > /files/f; echo same > /files/keep; echo c > /cache/c",
    );
    let s1 = save_to(w, p, a, "s1").await;
    sql(w, a, "update t set n = 2");
    files(a, "echo two > /files/f");
    (s1, save_to(w, p, a, "s2").await)
}

#[tokio::test]
#[ignore]
async fn live_service_strategies() {
    let Some((mut w, config)) = strategies_world() else {
        return;
    };
    let id = std::process::id();
    let (a, b) = (format!("t11_{id}_a"), format!("t11_{id}_b"));
    let p = w.part();
    w.projects
        .extend([format!("arena_{a}"), format!("arena_{b}")]);
    p.start(start(&a, config.clone(), None)).await.unwrap();
    let (s1, s2) = two_saves(&w, &p, &a).await;
    let inode = |d: &PathBuf| {
        std::os::unix::fs::MetadataExt::ino(&fs::metadata(d.join("files/keep")).unwrap())
    };
    assert_eq!(inode(&s1), inode(&s2), "an unchanged file is one inode");
    p.start(start(&b, config, Some("s1"))).await.unwrap();
    assert_eq!(sql(&w, &b, "select n from t"), "1");
    assert_eq!(files(&b, "cat /files/f; ls /cache | wc -l"), "one\n0");
    for i in [&a, &b] {
        p.stop(IdParams {
            instance: i.clone(),
        })
        .await
        .unwrap();
    }
    for s in ["s1", "s2"] {
        p.drop_save(SaveId { save: s.into() }).await.unwrap();
    }
}
