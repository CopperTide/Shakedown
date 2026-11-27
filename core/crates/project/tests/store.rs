//! A project in memory behaves as the same project in a folder: the same items and problems, and
//! the same files after a delete, a regroup and an undo.

use std::path::Path;
use std::sync::Arc;

use shakedown_project::format::Kind;
use shakedown_project::store::{Files, MemStore, Store};
use shakedown_project::{Id, Project, ProjectError};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/larkspur");

/// Every file under `dir`, by path relative to `root`.
fn read_tree(root: &Path, dir: &Path, files: &mut Files) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            read_tree(root, &path, files);
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(rel, Arc::from(std::fs::read(&path).unwrap()));
        }
    }
}

fn tree(root: &Path) -> Files {
    let mut files = Files::new();
    read_tree(root, root, &mut files);
    files
}

/// The fixture in a temporary folder, and the same files in memory.
fn both() -> (tempfile::TempDir, Project, Arc<MemStore>, Project) {
    let dir = tempfile::tempdir().unwrap();
    for (path, bytes) in tree(Path::new(FIXTURE)) {
        let file = dir.path().join(&path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, &*bytes).unwrap();
    }
    let on_disk = Project::open(dir.path()).unwrap();
    let mem = Arc::new(MemStore::new(tree(Path::new(FIXTURE))));
    let in_memory = Project::open_in(mem.clone() as Arc<dyn Store>).unwrap();
    (dir, on_disk, mem, in_memory)
}

fn id(s: &str) -> Id {
    Id::parse(s).unwrap()
}

/// The files of both, trash stamps (the delete's time) replaced by `<stamp>`.
fn same_files(dir: &Path, mem: &MemStore) {
    let unstamp = |files: Files| -> Vec<(String, Vec<u8>)> {
        let cut = |p: &str| match p.strip_prefix(".shakedown/trash/") {
            Some(rest) => format!("trash/<stamp>/{}", rest.split_once('/').map_or("", |r| r.1)),
            None => p.to_string(),
        };
        files
            .into_iter()
            .map(|(p, b)| (cut(&p), b.to_vec()))
            .collect()
    };
    assert_eq!(unstamp(tree(dir)), unstamp(mem.files()));
}

#[test]
fn the_same_items_and_problems() {
    let (_dir, on_disk, _mem, in_memory) = both();
    for kind in Kind::ALL {
        assert_eq!(on_disk.list(kind), in_memory.list(kind), "{kind:?}");
    }
    assert_eq!(on_disk.problems, in_memory.problems);
    assert_eq!(on_disk.config, in_memory.config);
    assert!(in_memory.root().is_none() && on_disk.root().is_some());
    let team = id("planning-team");
    let kept = in_memory.kept_folder(Kind::Setup, &team);
    assert_eq!(kept, on_disk.kept_folder(Kind::Setup, &team));
    assert_eq!(kept.as_deref(), Some("setups/planning-team/"));
}

#[test]
fn a_delete_and_its_undo_change_the_same_files() {
    let (dir, mut on_disk, mem, mut in_memory) = both();
    for p in [&mut on_disk, &mut in_memory] {
        p.delete(Kind::World, &id("larkspur-plant"), true, None)
            .unwrap();
    }
    same_files(dir.path(), &mem);
    assert!(mem
        .changes()
        .iter()
        .any(|(p, b)| p.ends_with("world.yaml") && b.is_none()));
    for p in [&mut on_disk, &mut in_memory] {
        p.restore(None).unwrap();
    }
    same_files(dir.path(), &mem);
    assert_eq!(on_disk.list(Kind::World), in_memory.list(Kind::World));
}

#[test]
fn a_regroup_and_a_new_project_write_the_same_files() {
    let (dir, mut on_disk, mem, mut in_memory) = both();
    for p in [&mut on_disk, &mut in_memory] {
        let group = p.list(Kind::World).iter().find_map(|w| match &w.body {
            shakedown_project::ItemBody::World(w) => w.group.clone(),
            _ => None,
        });
        let _ = p.rename_group(group.as_deref().unwrap_or("none"), Some("acme"));
    }
    same_files(dir.path(), &mem);
    let fresh = tempfile::tempdir().unwrap();
    let mem = Arc::new(MemStore::default());
    Project::init(fresh.path(), "P", false).unwrap();
    Project::init_in(mem.clone() as Arc<dyn Store>, "P", false).unwrap();
    same_files(fresh.path(), &mem);
    let no_git = Project::init_in(Arc::new(MemStore::default()), "P", true);
    assert!(
        matches!(no_git, Err(ProjectError::NoGitFolder)),
        "git needs a folder"
    );
}
