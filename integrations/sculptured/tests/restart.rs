//! A program restarted while an instance runs reaches it by its record (`.live/<id>/run.json`): it
//! still inspects, saves and stops it. The instance is a stand-in: a process on the instance's data
//! folder, as serve names it, and a loopback server answering CopperTide's changes route.
use serde_json::json;
use shakedown_part::proto::{IdParams, InspectParams, SaveParams, Target};
use shakedown_part::Part;
use shakedown_sculptured::Sculptured;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// The test's folder, removed on drop (with the process the test started).
struct Tmp(PathBuf, Option<Child>);

impl Drop for Tmp {
    fn drop(&mut self) {
        if let Some(c) = &mut self.1 {
            let group = format!("-{}", c.id());
            let _ = Command::new("kill").args(["-KILL", "--", &group]).status();
            let _ = c.wait();
        }
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A `sh` in a group of its own whose `$0` is `arg`, as serve names its data folder.
fn spawn(arg: &Path) -> Child {
    Command::new("/bin/sh")
        .args(["-c", "sleep 300; :"])
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap()
}

/// A server on a fresh loopback port answering every request with one change; its port.
fn coppertide() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let body = json!({"changes": [{"id": "chg-1", "action": "buy", "system": "erp", "summary": "s",
                                   "payload": {}, "reason": "r", "decision": null}]})
    .to_string();
    std::thread::spawn(move || {
        for s in listener.incoming() {
            let mut s = s.unwrap();
            let mut got = vec![0u8; 8192];
            let _ = s.read(&mut got);
            let answer = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = s.write_all(answer.as_bytes());
        }
    });
    port
}

/// Records instance `id` under `saves` as running `/bin/sh` with serve's pid `pid` on `port`.
fn record(saves: &Path, id: &str, pid: u32, port: u16, tmp: &Path) {
    let folder = saves.join(".live").join(id);
    fs::create_dir_all(folder.join("data")).unwrap();
    fs::create_dir_all(tmp).unwrap();
    let run = json!({"config": {"binary": "/bin/sh", "team": "shakedown"}, "binary": "/bin/sh",
                     "build": "b1", "port": port, "tmp": tmp,
                     "pids": {"serve": pid, "pgid": pid, "hostd": null}});
    fs::write(folder.join("run.json"), run.to_string()).unwrap();
}

/// What inspecting running instance `a` gives.
async fn inspected(part: &Sculptured) -> shakedown_part::proto::Docs {
    let target = Target::Instance {
        instance: "a".into(),
    };
    let docs = part.inspect(InspectParams {
        target,
        keys: vec![],
    });
    docs.await.unwrap()
}

/// Saves running instance `a` as `s1` in `dir`.
async fn saved(part: &Sculptured, dir: &Path) -> shakedown_part::proto::Saved {
    let saved = part.save(SaveParams {
        instance: "a".into(),
        save: "s1".into(),
        dir: dir.to_string_lossy().into(),
    });
    saved.await.unwrap()
}

#[tokio::test]
async fn a_restarted_program_still_inspects_saves_and_stops_a_running_instance() {
    let root = std::env::temp_dir().join(format!("shakedown_sc_restart_{}", std::process::id()));
    let mut t = Tmp(root.clone(), None);
    let saves = root.join("saves/sculptured");
    let live = saves.join(".live");
    let server = spawn(&live.join("a/data"));
    let (pid, scratch) = (server.id(), root.join("scratch"));
    t.1 = Some(server);
    record(&saves, "a", pid, coppertide(), &scratch);
    let part = Sculptured::new(saves.clone()); // nothing in memory: only the record
    assert_eq!(inspected(&part).await.docs["change/chg-1"]["action"], "buy");
    let dir = saves.join("s1");
    assert!(saved(&part, &dir).await.exact);
    let changes = fs::read_to_string(dir.join("changes.json")).unwrap();
    assert!(changes.contains("chg-1"));
    let stop = part.stop(IdParams {
        instance: "a".into(),
    });
    stop.await.unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let exited = t.1.as_mut().unwrap().try_wait().unwrap().is_some();
    assert!(exited, "the instance's server is stopped");
    assert!(!live.join("a").exists() && !scratch.exists());
}
