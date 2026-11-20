//! The fake `docker` the service tests run against, and the params they start and save with.
#![allow(dead_code)] // each test file uses its own share of these
use arena_part::proto::*;
use arena_service::Service;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The fake: logs its arguments (one line a call) to `log` beside its `bin`, and fails where the environment says so. `port` answers the
/// n-th of `FAKE_PORTS` after the n-th `up` or `start` (a restarted service gets a new port). A
/// compose command run with `IMAGE_*` or `SHAKEDOWN_CLOCK_DIR` set logs them first, as `env: <sorted
/// variables>`; `ps` of one service answers `FAKE_HEALTH` (default `running healthy`), of every
/// service `FAKE_PS` (tab-separated service, state, health and exit code lines).
pub const FAKE: &str = r#"#!/bin/sh
LOG="$(dirname "$0")/../log"
vars=$(env | grep -E '^(IMAGE_|SHAKEDOWN_CLOCK_DIR=)' | sort | tr '\n' ' ')
case "$1:$vars" in compose:?*) echo "env: $vars" >> "$LOG" ;; esac
{ printf '%s' "$*" | tr '\n' ' '; echo; } >> "$LOG"
case "$*" in
  *" ps "*) case "$*" in
             *Service*) printf '%b\n' "${FAKE_PS:-web\trunning\thealthy\t0}" ;;
             *) echo "${FAKE_HEALTH:-running healthy}" ;;
           esac ;;
  *" stop") [ -n "$FAKE_STOP_FAIL" ] && { echo no stop >&2; exit 1; } ;;
  *" up "*) [ -n "$FAKE_UP_FAIL" ] && { echo no up >&2; exit 1; } ;;
  *-cpf*) case "$*" in *_bad:/v*) echo no export >&2; exit 1 ;; *) printf tar ;; esac ;;
  *" port "*) n=$(grep -cE ' (start|up)( |$)' "$LOG")
             echo "127.0.0.1:$(echo "${FAKE_PORTS:-49153}" | awk -v n="$n" '{print $(n < NF ? n : NF)}')" ;;
  *pg_dump*) printf -- '-- dumped at some time\nCREATE TABLE t (n int);\n' ;;
esac
exit 0
"#;

/// A temporary folder with the fake docker; removed on drop.
pub struct Fake(pub PathBuf);

impl Fake {
    pub fn new(tag: &str) -> Fake {
        let dir = std::env::temp_dir().join(format!("arena_fake_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bin")).unwrap();
        let docker = dir.join("bin/docker");
        fs::write(&docker, FAKE).unwrap();
        fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(dir.join("compose.yml"), "services: {}\n").unwrap();
        Fake(dir)
    }

    pub fn part(&self) -> Service {
        Service::new(self.0.join("saves/service"))
    }

    /// The live package's part (world clock, deploys, watches) on the same folders.
    pub fn live_part(&self) -> Service {
        Service::live(self.0.join("saves/service"))
    }

    /// A config whose compose runs go to the fake; `extra` adds environment variables.
    pub fn config(&self, volumes: &[&str], extra: Value) -> Value {
        let path = format!(
            "{}:{}",
            self.0.join("bin").display(),
            std::env::var("PATH").unwrap()
        );
        let mut env = json!({"PATH": path});
        env.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json!({"compose_file": self.0.join("compose.yml"), "volumes": volumes, "env": env})
    }

    pub fn log(&self) -> String {
        fs::read_to_string(self.0.join("log")).unwrap_or_default()
    }

    pub fn live_file(&self, id: &str) -> PathBuf {
        self.0.join(format!("saves/service/.live/{id}.json"))
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn start(id: &str, config: Value) -> StartParams {
    StartParams {
        instance: id.into(),
        config,
        seed: 1,
        clock_ms: 0,
        from: None,
        world_time: None,
    }
}

pub fn save_params(f: &Fake, id: &str) -> SaveParams {
    let dir = f.0.join("saves/service/s1");
    SaveParams {
        instance: id.into(),
        save: "s1".into(),
        dir: dir.to_string_lossy().into(),
    }
}

/// A config with `volumes` given as JSON (names or specs) and `extra` top-level fields.
pub fn config_with(f: &Fake, volumes: Value, extra: Value) -> Value {
    let mut c = f.config(&[], json!({}));
    c["volumes"] = volumes;
    c.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    c
}

pub fn save_as(f: &Fake, id: &str, save: &str) -> SaveParams {
    let dir = f.0.join("saves/service").join(save);
    SaveParams {
        instance: id.into(),
        save: save.into(),
        dir: dir.to_string_lossy().into(),
    }
}

/// The log lines written since `from` lines.
pub fn log_since(f: &Fake, from: usize) -> Vec<String> {
    f.log().lines().skip(from).map(str::to_string).collect()
}

/// A small HTTP server on 127.0.0.1 standing in for every probed service: each request's path
/// (with its query) is answered from `answers` (status, body), else 404; each request's head is
/// kept in `heads`. It serves until the test program ends.
pub struct Probed {
    pub port: u16,
    pub answers: Arc<Mutex<HashMap<String, (u16, String)>>>,
    pub heads: Arc<Mutex<Vec<String>>>,
}

impl Probed {
    pub fn new() -> Probed {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let (answers, heads) = (Arc::default(), Arc::default());
        let (a, h) = (Arc::clone(&answers), Arc::clone(&heads));
        std::thread::spawn(move || {
            for conn in l.incoming().flatten() {
                answer_one(conn, &a, &h);
            }
        });
        Probed {
            port,
            answers,
            heads,
        }
    }

    pub fn answer(&self, path: &str, status: u16, body: &str) {
        let mut a = self.answers.lock().unwrap();
        a.insert(path.to_string(), (status, body.to_string()));
    }
}

fn answer_one(
    mut conn: TcpStream,
    answers: &Mutex<HashMap<String, (u16, String)>>,
    heads: &Mutex<Vec<String>>,
) {
    let mut head = String::new();
    let mut reader = BufReader::new(conn.try_clone().unwrap());
    while reader.read_line(&mut head).unwrap_or(0) > 2 && !head.ends_with("\r\n\r\n") {}
    let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
    heads.lock().unwrap().push(head);
    let (status, body) =
        (answers.lock().unwrap().get(&path).cloned()).unwrap_or((404, "not found".into()));
    let reply = format!(
        "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = conn.write_all(reply.as_bytes());
}
