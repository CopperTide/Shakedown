//! Instance bookkeeping: records of running instances on disk (a restarted program reads them), the
//! table of instances in memory, and a lock per key.
use std::path::PathBuf;
use std::sync::Arc;

use arena_part::helpers::instances::{KeyedLocks, LiveRecords, Registry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Run {
    port: u16,
}

/// A fresh folder for one test, removed on drop.
struct Tmp(PathBuf);

impl Tmp {
    fn new(tag: &str) -> Tmp {
        let dir = std::env::temp_dir().join(format!("arena_inst_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Tmp(dir)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_record_is_one_json_file_per_instance() {
    let t = Tmp::new("files");
    let live: LiveRecords<Run> = LiveRecords::files(t.0.join(".live"));
    assert_eq!((live.get("a"), live.ids()), (None, vec![]));
    live.record("a", &Run { port: 7 }).unwrap();
    let file = t.0.join(".live/a.json");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), r#"{"port":7}"#);
    std::fs::write(t.0.join(".live/junk.txt"), "x").unwrap();
    assert_eq!(live.get("a"), Some(Run { port: 7 }));
    assert_eq!(live.ids(), ["a"]);
    live.forget("a");
    live.forget("a");
    assert!(!file.exists() && live.get("a").is_none());
}

#[test]
fn a_record_may_be_a_file_in_the_instances_folder() {
    let t = Tmp::new("folders");
    let live: LiveRecords<Run> = LiveRecords::folders(t.0.clone(), "run.json");
    live.record("a", &Run { port: 9 }).unwrap();
    std::fs::create_dir_all(t.0.join("Not-An-Id")).unwrap();
    assert!(t.0.join("a/run.json").is_file());
    assert_eq!(
        (live.get("a"), live.ids()),
        (Some(Run { port: 9 }), vec!["a".to_string()])
    );
    assert_eq!(live.path("b"), t.0.join("b/run.json"));
    std::fs::write(t.0.join("a/run.json"), "not json").unwrap();
    assert_eq!(live.get("a"), None, "a broken record is no record");
}

#[test]
fn recording_fails_when_the_folder_cannot_be_made() {
    let t = Tmp::new("blocked");
    std::fs::create_dir_all(&t.0).unwrap();
    std::fs::write(t.0.join(".live"), "in the way").unwrap();
    let live: LiveRecords<Run> = LiveRecords::files(t.0.join(".live"));
    assert!(live.record("a", &Run { port: 1 }).is_err());
}

#[test]
fn the_registry_holds_the_running_instances() {
    let r: Registry<u32> = Registry::default();
    r.insert("a".into(), 1);
    assert!(r.contains("a") && !r.contains("b"));
    assert!(r.update("a", |n| *n += 1) && !r.update("b", |_| unreachable!()));
    assert_eq!((r.get("a"), r.get("b")), (Some(2), None));
    assert_eq!(r.lock().values().sum::<u32>(), 2);
    assert_eq!((r.remove("a"), r.remove("a")), (Some(2), None));
}

#[tokio::test]
async fn one_lock_per_key_until_it_is_forgotten() {
    let locks = KeyedLocks::default();
    let (a, a2, b) = (
        locks.lock_for("a"),
        locks.lock_for("a"),
        locks.lock_for("b"),
    );
    assert!(Arc::ptr_eq(&a, &a2) && !Arc::ptr_eq(&a, &b));
    let held = a.lock().await;
    assert!(a2.try_lock().is_err() && b.try_lock().is_ok());
    drop(held);
    locks.forget("a");
    assert!(!Arc::ptr_eq(&a, &locks.lock_for("a")));
}
