//! Both stores behave alike: every check here runs on a folder and in memory.

use std::io::ErrorKind;
use std::sync::Arc;

use super::*;

/// The same check on an empty folder and an empty memory store.
fn on_both(check: impl Fn(&dyn Store)) {
    let dir = tempfile::tempdir().unwrap();
    check(&FsStore::new(dir.path()));
    check(&MemStore::default());
}

fn names(store: &dyn Store, dir: &str) -> Vec<(String, bool, u64)> {
    let mut found: Vec<_> = (store.list(dir).unwrap().into_iter())
        .map(|e| (e.name, e.dir, e.size))
        .collect();
    found.sort();
    found
}

#[test]
fn writes_read_back_and_imply_their_folders() {
    on_both(|s| {
        s.write("worlds/mill/world.yaml", b"label: Mill\n").unwrap();
        s.write("worlds/mill/data/orders.csv", b"a,b\n").unwrap();
        s.write("shakedown.yaml", b"x").unwrap();
        assert_eq!(s.read("worlds/mill/world.yaml").unwrap(), b"label: Mill\n");
        let top = names(s, "");
        assert_eq!(
            top,
            [
                ("shakedown.yaml".into(), false, 1),
                ("worlds".into(), true, 0)
            ]
        );
        let mill = names(s, "worlds/mill");
        assert_eq!(
            mill,
            [("data".into(), true, 0), ("world.yaml".into(), false, 12)]
        );
        assert!(s.is_dir("worlds/mill") && s.is_file("worlds/mill/data/orders.csv"));
        assert!(!s.exists("worlds/mil") && !s.exists("worlds/mill/world"));
        assert_eq!(s.list("nowhere").unwrap_err().kind(), ErrorKind::NotFound);
        assert_eq!(
            s.read("nowhere.yaml").unwrap_err().kind(),
            ErrorKind::NotFound
        );
    });
}

#[test]
fn rename_remove_and_remove_dir() {
    on_both(|s| {
        s.write("a/one.yaml", b"1").unwrap();
        s.write("a/two.yaml", b"2").unwrap();
        s.write("b.yaml", b"b").unwrap();
        s.rename("b.yaml", "c/d/b.yaml").unwrap();
        assert!(!s.exists("b.yaml") && s.read("c/d/b.yaml").unwrap() == b"b");
        s.rename("a/one.yaml", "a/two.yaml").unwrap();
        assert_eq!(s.read("a/two.yaml").unwrap(), b"1");
        s.remove("a/two.yaml").unwrap();
        assert_eq!(
            s.remove("a/two.yaml").unwrap_err().kind(),
            ErrorKind::NotFound
        );
        s.remove_dir("c").unwrap();
        assert!(!s.exists("c/d/b.yaml"));
        assert_eq!(
            s.rename("gone", "x").unwrap_err().kind(),
            ErrorKind::NotFound
        );
    });
}

#[test]
fn a_write_never_lands_on_a_folder_or_below_a_file() {
    on_both(|s| {
        s.write("a/b.yaml", b"1").unwrap();
        assert!(s.write("a", b"x").is_err());
        assert!(s.write("a/b.yaml/c", b"x").is_err());
        assert_eq!(s.read("a/b.yaml").unwrap(), b"1");
    });
}

#[test]
fn paths_that_leave_the_project_are_refused() {
    on_both(|s| {
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "a//b",
            "./a",
            "a\\b",
            "a\0b",
        ] {
            assert_eq!(
                s.write(bad, b"x").unwrap_err().kind(),
                ErrorKind::InvalidInput,
                "{bad:?}"
            );
            assert!(s.read(bad).is_err() && s.stat(bad).is_none(), "{bad:?}");
        }
        assert!(check_path(&"a".repeat(MAX_PATH + 1)).is_err());
        assert!(check_path("worlds/mill/data/x.csv").is_ok());
    });
}

#[test]
fn read_head_cuts_and_tells_the_size() {
    on_both(|s| {
        s.write("d/big.txt", b"0123456789").unwrap();
        assert_eq!(
            s.read_head("d", "big.txt", 4).unwrap(),
            (b"0123".to_vec(), 10)
        );
    });
}

#[test]
fn memory_collects_what_changed() {
    let mut files = Files::new();
    files.insert("keep.yaml".into(), Arc::from(&b"k"[..]));
    files.insert("old.yaml".into(), Arc::from(&b"o"[..]));
    let s = MemStore::new(files);
    assert!(s.changes().is_empty());
    s.write("new.yaml", b"n").unwrap();
    s.rename("old.yaml", "moved.yaml").unwrap();
    s.write("tmp.yaml", b"t").unwrap();
    s.remove("tmp.yaml").unwrap();
    let changes: Vec<(String, Option<Vec<u8>>)> = (s.changes().into_iter())
        .map(|(p, b)| (p, b.map(|b| b.to_vec())))
        .collect();
    let expected = [
        ("moved.yaml", Some(b"o".to_vec())),
        ("new.yaml", Some(b"n".to_vec())),
        ("old.yaml", None),
        ("tmp.yaml", None),
    ];
    let expected: Vec<_> = expected
        .into_iter()
        .map(|(p, b)| (p.to_string(), b))
        .collect();
    assert_eq!(changes, expected);
    assert_eq!(s.files().len(), 3);
}

#[test]
fn a_folder_link_out_of_the_project_is_not_read() {
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "s").unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("d")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), dir.path().join("d/out")).unwrap();
    let s = FsStore::new(dir.path());
    let err = s.read_head("d", "out/secret.txt", 10).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(s.list("d").unwrap()[0].link && s.stat("d/out").unwrap().link);
}

#[test]
fn yaml_and_json_files_parse_for_an_index() {
    let world = parsed("worlds/mill/world.yaml", b"label: Mill\nn: 2\n").unwrap();
    assert_eq!(world, serde_json::json!({"label": "Mill", "n": 2}));
    assert_eq!(parsed("a.json", b"[1]"), Some(serde_json::json!([1])));
    assert_eq!(parsed("a.csv", b"a,b"), None);
    assert_eq!(parsed("a.yaml", b"key: [\n"), None);
    assert_eq!(parsed("a.json", b"\"a\\u0000b\""), None);
}
