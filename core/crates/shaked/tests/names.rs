//! Both names of the service start and answer: `shaked`, and `arenad`, its name until 2026-11-19
//! (`core/crates/arenad-alias` builds the same `main.rs` under it, for scripts and programs that
//! still run `arenad`).
//!
//! Each runs as a real process on a toy world of its own (its own packages, saves and worlds
//! folders), so its startup cleanup touches nothing but those. Like any shaked it opens the
//! metadata store on `SHAKEDOWN_PG_URL`; never run a shaked of your own there while tests run.
mod support;

use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use support::{
    packages::{self, toy_world},
    pg::{self, pg_url},
    run,
};

/// The `arenad` program next to `shaked`, built (`cargo build -p arenad`) in the same profile.
fn arenad_binary() -> PathBuf {
    let shaked = Path::new(env!("CARGO_BIN_EXE_shaked"));
    let release = shaked.parent().and_then(|d| d.file_name()) == Some("release".as_ref());
    let mut build = Command::new("cargo");
    build.args(["build", "-q", "-p", "arenad"]);
    if release {
        build.arg("--release");
    }
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let st = build.current_dir(core).status().unwrap();
    assert!(st.success(), "building arenad failed");
    shaked.with_file_name("arenad")
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// A started service; stopped with ctrl-c (then killed) and its folders removed on drop.
struct Service {
    child: Child,
    dirs: Vec<PathBuf>,
}

impl Drop for Service {
    fn drop(&mut self) {
        let pid = i32::try_from(self.child.id()).expect("a pid fits in i32");
        // SAFETY: kill(2) on the pid of a child this test started and has not reaped yet.
        unsafe { libc::kill(pid, libc::SIGINT) };
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        for d in &self.dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

/// `binary` started on the toy world in `packages`, with saves and worlds under the temp folder
/// and its records in the scratch database `db` (never the shared default one).
fn start(binary: &Path, tag: &str, port: u16, packages_root: &Path, db: &str) -> Service {
    let base = std::env::temp_dir().join(format!("shaked-names-{tag}-{}", std::process::id()));
    let (saves, worlds) = (base.join("saves"), base.join("worlds"));
    let child = Command::new(binary)
        .env("SHAKEDOWN_TEST_GUARD", "1")
        .env("SHAKEDOWN_DB", db)
        .env("SHAKEDOWN_PG_URL", pg_url())
        .env("SHAKEDOWN_PORT", port.to_string())
        .env("SHAKEDOWN_PROXY_PORT", "0")
        .env("SHAKEDOWN_PACKAGES", packages_root)
        .env("SHAKEDOWN_BENCHMARK", "toybench")
        .env("SHAKEDOWN_BIN_DIR", packages::bin_dir())
        .env("SHAKEDOWN_SAVES", &saves)
        .env("SHAKEDOWN_WORLDS", &worlds)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Service {
        child,
        dirs: vec![base],
    }
}

/// `/api/health` of the service on `port` once it answers (60 s at most): the benchmark's name.
async fn health_benchmark(service: &mut Service, port: u16) -> String {
    let url = format!("http://127.0.0.1:{port}/api/health");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        assert!(
            service.child.try_wait().unwrap().is_none(),
            "the service exited while starting"
        );
        if let Ok(r) = reqwest::get(&url).await {
            let v: serde_json::Value = r.json().await.unwrap();
            return v["benchmark"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
        }
        assert!(Instant::now() < deadline, "no answer on {url} within 60 s");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[test]
fn shaked_and_its_old_name_arenad_both_start_and_answer() {
    let arenad = arenad_binary();
    let shaked = PathBuf::from(env!("CARGO_BIN_EXE_shaked"));
    let root = toy_world("names", &[], &[]);
    let clean = pg::Cleanup::new();
    for (tag, binary) in [("shaked", &shaked), ("arenad", &arenad)] {
        let port = free_port();
        let db = pg::db_name(&format!("names_{tag}"));
        clean.add(&db);
        let mut service = start(binary, tag, port, &root.0, &db);
        let bench = run(health_benchmark(&mut service, port));
        assert_eq!(bench, "toybench", "{tag}");
    }
}

/// A test-started shaked with no database of its own refuses the shared default one (`shakedown`,
/// or `arena` on an older server) before it connects. The server URL points nowhere, so a guard
/// that failed could not reach anyone's records either.
#[test]
fn a_test_started_shaked_refuses_the_shared_default_database() {
    let shaked = PathBuf::from(env!("CARGO_BIN_EXE_shaked"));
    let out = Command::new(shaked)
        .env("SHAKEDOWN_TEST_GUARD", "1")
        .env_remove("SHAKEDOWN_DB")
        .env_remove("SHAKEDOWN_ALLOW_DEFAULT_DB")
        .env(
            "SHAKEDOWN_PG_URL",
            "postgres://nobody:nothing@127.0.0.1:9/postgres",
        )
        .env("SHAKEDOWN_PORT", free_port().to_string())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("SHAKEDOWN_DB"), "{err}");
}

/// Every test that starts a shaked binary sets the guard (and so names a database of its own).
#[test]
fn every_test_that_starts_shaked_sets_the_guard() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for e in std::fs::read_dir(&dir).unwrap() {
        let path = e.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        if text.contains(concat!("CARGO_BIN", "_EXE_")) {
            let guarded = text.contains("SHAKEDOWN_TEST_GUARD");
            assert!(
                guarded,
                "{} starts shaked without the guard",
                path.display()
            );
        }
    }
}

/// `--hosted` opens the hosted store (no more "not available"): with nowhere to connect, it stops
/// naming the database it could not reach, before serving anything.
#[test]
fn hosted_mode_opens_its_three_logins() {
    let shaked = PathBuf::from(env!("CARGO_BIN_EXE_shaked"));
    let nowhere = "postgres://nobody:nothing@127.0.0.1:9/postgres";
    let out = Command::new(shaked)
        .arg("--hosted")
        .env("SHAKEDOWN_TEST_GUARD", "1")
        .env("SHAKEDOWN_PG_URL", nowhere)
        .env("SHAKEDOWN_SYSTEM_PG_URL", nowhere)
        .env("SHAKEDOWN_MIGRATE_PG_URL", nowhere)
        .env("SHAKEDOWN_PORT", free_port().to_string())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("not available in this build"), "{err}");
    assert!(err.contains("connecting to migrate"), "{err}");
}
