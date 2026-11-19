//! The tar digest: order-free, sensitive to content and metadata.
#![cfg(feature = "compose")]
use arena_part::helpers::tar_digest::digest;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A temporary folder removed on drop, even when the test fails.
struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn tar(dir: &Path, out: &str, names: &[&str]) -> PathBuf {
    let path = dir.join(out);
    let status = Command::new("tar")
        .arg("-C")
        .arg(dir.join("src"))
        .arg("-cf")
        .arg(&path)
        .args(names)
        .status()
        .unwrap();
    assert!(status.success());
    path
}

#[test]
fn digest_ignores_order_and_sees_changes() {
    let t = Tmp(std::env::temp_dir().join(format!("arena_kit_tar_{}", std::process::id())));
    fs::create_dir_all(t.0.join("src")).unwrap();
    fs::write(t.0.join("src/a"), "aaaa").unwrap();
    fs::write(t.0.join("src/b"), "bb").unwrap();
    let ab = digest(&tar(&t.0, "ab.tar", &["a", "b"])).unwrap();
    let ba = digest(&tar(&t.0, "ba.tar", &["b", "a"])).unwrap();
    assert_eq!((ab.files, ab.bytes), (2, 6));
    assert_eq!(ab, ba);
    fs::write(t.0.join("src/b"), "cc").unwrap();
    assert_ne!(
        digest(&tar(&t.0, "c.tar", &["a", "b"])).unwrap().hash,
        ab.hash
    );
    let touched = Command::new("touch")
        .args(["-d", "@1700000000"])
        .arg(t.0.join("src/a"))
        .status()
        .unwrap();
    assert!(touched.success());
    let moved = digest(&tar(&t.0, "d.tar", &["a"])).unwrap();
    assert_ne!(
        moved.hash,
        digest(&tar(&t.0, "e.tar", &["b"])).unwrap().hash
    );
}

#[test]
fn digest_reader_matches_digest() {
    use arena_part::helpers::tar_digest::digest_reader;
    let t = Tmp(std::env::temp_dir().join(format!("arena_kit_tar_reader_{}", std::process::id())));
    fs::create_dir_all(t.0.join("src")).unwrap();
    fs::write(t.0.join("src/a"), "aaaa").unwrap();
    let path = tar(&t.0, "a.tar", &["a"]);
    let from_reader = digest_reader(fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(from_reader, digest(&path).unwrap());
    assert_eq!(from_reader.files, 1);
}

#[test]
fn digest_ignores_when_a_link_was_made_but_not_where_it_points() {
    let t = Tmp(std::env::temp_dir().join(format!("arena_kit_tar_link_{}", std::process::id())));
    fs::create_dir_all(t.0.join("src")).unwrap();
    let link = t.0.join("src/l");
    std::os::unix::fs::symlink("a", &link).unwrap();
    let touch = |at: &str| {
        let s = Command::new("touch")
            .args(["-h", "-d", at])
            .arg(&link)
            .status();
        assert!(s.unwrap().success());
    };
    touch("@1700000000");
    let early = digest(&tar(&t.0, "early.tar", &["l"])).unwrap();
    touch("@1800000000");
    assert_eq!(digest(&tar(&t.0, "late.tar", &["l"])).unwrap(), early);
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("b", &link).unwrap();
    touch("@1800000000");
    assert_ne!(
        digest(&tar(&t.0, "b.tar", &["l"])).unwrap().hash,
        early.hash
    );
}
