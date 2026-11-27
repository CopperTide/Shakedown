//! A world's own data, delivered to its runs (docs/protocol.md, "World data"). The data sources
//! a world marks "Every run starts from this" (`default_state`, a `file`) are read from the
//! project's world store when an experiment is deployed, kept as a snapshot named after their
//! digest (`<saves>/_world-data/<digest16>/`, the files at their paths in the world's folder),
//! and given to every part of the run's world in its `start` config under [`KEY`]:
//!
//! ```json
//! {"world_data": {"digest": "<sha256>", "dir": "/abs/snapshot", "world": "airline",
//!                 "files": [{"path": "data/db.json", "seeds": "airline", "size": 5318906, "sha256": "..."}]}}
//! ```
//!
//! The benchmark gets the same object in `gold` and `score`. Applied to a benchmark's
//! description ([`apply`]), the data also names its base and gold saves (the version gets
//! `-data-<digest12>`) and joins each part's recorded fingerprint ([`fingerprint`]), so saves,
//! restores and forks never mix two versions of the data. A run records its world's configs, so
//! a fork reads the data back from them ([`of_world`]). A world with no such files is run
//! exactly as before.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use parts::proto::{BenchDescribe, WorldPart};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use shakedown_project::format::WorldFile;
use shakedown_project::store::Store;

/// The key of a part's `start` config (and of `gold` and `score` params) that holds the data.
pub const KEY: &str = "world_data";
/// The folder under a hub's saves where snapshots are kept (`_` keeps it apart from packages).
const SNAPSHOTS: &str = "_world-data";
/// The largest data file a world takes, on every backend (a hosted project file is one row).
pub const MAX_FILE: u64 = 10 << 20;

/// One file of a world's data, read from its store.
struct File {
    path: String,
    seeds: Option<String>,
    bytes: Vec<u8>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The files of `world` (id `id`) every run starts from, read from `store`.
fn files(store: &dyn Store, id: &str, world: &WorldFile) -> Result<Vec<File>> {
    let mut out = vec![];
    for source in world.data.iter().filter(|d| d.default_state) {
        let Some(path) = &source.file else { continue };
        let at = format!("worlds/{id}/{path}");
        let bytes = store
            .read(&at)
            .with_context(|| format!("the world {id} has no data file {path}: upload it"))?;
        out.push(File {
            path: path.clone(),
            seeds: source.seeds.clone(),
            bytes,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// The digest of a world's data: SHA-256 over each file's path and SHA-256, in path order.
fn digest(files: &[(String, String)]) -> String {
    let mut h = Sha256::new();
    for (path, sha) in files {
        h.update(path.as_bytes());
        h.update([0]);
        h.update(sha.as_bytes());
        h.update([0]);
    }
    hex(&h.finalize())
}

/// The data the runs of world `id` start from, as [`KEY`] gives it to parts, with its snapshot
/// kept under `saves` (written once per digest). None when the world marks no file.
pub fn snapshot(
    store: &dyn Store,
    id: &str,
    world: &WorldFile,
    saves: &Path,
) -> Result<Option<Value>> {
    let files = files(store, id, world)?;
    if files.is_empty() {
        return Ok(None);
    }
    let shas: Vec<(String, String)> = (files.iter())
        .map(|f| (f.path.clone(), hex(&Sha256::digest(&f.bytes))))
        .collect();
    let digest = digest(&shas);
    let dir = saves.join(SNAPSHOTS).join(&digest[..16]);
    if !dir.is_dir() {
        write_snapshot(&dir, &files)?;
    }
    let listed: Vec<Value> = (files.iter().zip(&shas))
        .map(|(f, (_, sha))| json!({"path": f.path, "seeds": f.seeds, "size": f.bytes.len(), "sha256": sha}))
        .collect();
    Ok(Some(
        json!({"digest": digest, "dir": dir.display().to_string(), "world": id, "files": listed}),
    ))
}

/// Writes the snapshot into a new folder beside `dir`, then moves it into place.
fn write_snapshot(dir: &Path, files: &[File]) -> Result<()> {
    let parent = dir.parent().context("a snapshot folder has a parent")?;
    std::fs::create_dir_all(parent)?;
    let tmp: PathBuf = parent.join(format!(".tmp-{}", uuid::Uuid::new_v4().simple()));
    for f in files {
        let to = tmp.join(&f.path);
        std::fs::create_dir_all(to.parent().unwrap_or(&tmp))?;
        std::fs::write(&to, &f.bytes).with_context(|| format!("writing {}", to.display()))?;
    }
    if let Err(e) = std::fs::rename(&tmp, dir) {
        let _ = std::fs::remove_dir_all(&tmp);
        if !dir.is_dir() {
            bail!("keeping the world's data at {}: {e}", dir.display());
        }
    }
    Ok(())
}

/// `bench` with `data` in every part's config, and its version naming the data (so its base and
/// gold saves are the data's own). Refused when the snapshot is gone.
pub fn apply(bench: &mut BenchDescribe, data: &Value) -> Result<()> {
    let dir = data["dir"].as_str().unwrap_or_default();
    if !Path::new(dir).is_dir() {
        bail!("the world's data snapshot {dir} is gone: deploy the experiment again");
    }
    let digest = data["digest"].as_str().unwrap_or_default();
    bench.version = format!("{}-data-{}", bench.version, &digest[..digest.len().min(12)]);
    for part in &mut bench.world {
        let mut config = match std::mem::take(&mut part.config) {
            Value::Object(m) => m,
            Value::Null => Map::new(),
            other => bail!(
                "the part {} has a config that is not an object: {other}",
                part.part
            ),
        };
        config.insert(KEY.into(), data.clone());
        part.config = Value::Object(config);
    }
    Ok(())
}

/// The data a recorded world's parts were given, if any.
pub fn of_world(world: &[WorldPart]) -> Option<&Value> {
    world.iter().find_map(|p| p.config.get(KEY))
}

/// A part's `fingerprint` with the digest of the data in its `config`, if it was given any.
pub fn fingerprint(fp: Value, config: &Value) -> Value {
    let Some(digest) = config.get(KEY).and_then(|d| d.get("digest")).cloned() else {
        return fp;
    };
    match fp {
        Value::Object(mut m) => {
            m.insert(KEY.into(), digest);
            Value::Object(m)
        }
        other => json!({"part": other, KEY: digest}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakedown_project::store::MemStore;

    fn world(yaml: &str) -> WorldFile {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    const WORLD: &str = "format: shakedown-world/3\nlabel: W\npackage: p\ndata:\n- file: data/db.json\n  seeds: desk\n  default_state: true\n- file: data/notes.txt\n";

    fn store(db: &str) -> MemStore {
        let s = MemStore::default();
        s.write("worlds/w/data/db.json", db.as_bytes()).unwrap();
        s.write("worlds/w/data/notes.txt", b"not a start file")
            .unwrap();
        s
    }

    #[test]
    fn the_marked_files_are_kept_by_digest_and_a_change_is_a_new_digest() {
        let saves = tempfile::tempdir().unwrap();
        let w = world(WORLD);
        let one = snapshot(&store("{\"a\": 1}"), "w", &w, saves.path())
            .unwrap()
            .unwrap();
        assert_eq!(one["files"].as_array().unwrap().len(), 1, "{one}");
        assert_eq!(one["files"][0]["seeds"], "desk");
        let dir = PathBuf::from(one["dir"].as_str().unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.join("data/db.json")).unwrap(),
            "{\"a\": 1}"
        );
        let same = snapshot(&store("{\"a\": 1}"), "w", &w, saves.path())
            .unwrap()
            .unwrap();
        assert_eq!(same["digest"], one["digest"]);
        let two = snapshot(&store("{\"a\": 2}"), "w", &w, saves.path())
            .unwrap()
            .unwrap();
        assert_ne!(two["digest"], one["digest"]);
        assert_ne!(two["dir"], one["dir"]);
    }

    #[test]
    fn a_world_without_start_files_has_no_data_and_a_missing_file_is_refused() {
        let saves = tempfile::tempdir().unwrap();
        let none = world("format: shakedown-world/3\nlabel: W\n");
        assert_eq!(
            snapshot(&store("{}"), "w", &none, saves.path()).unwrap(),
            None
        );
        let empty = MemStore::default();
        let err = snapshot(&empty, "w", &world(WORLD), saves.path()).unwrap_err();
        assert!(format!("{err:#}").contains("upload it"), "{err:#}");
    }

    #[test]
    fn applied_the_data_reaches_every_part_names_the_saves_and_joins_the_fingerprint() {
        let saves = tempfile::tempdir().unwrap();
        let data = snapshot(&store("{}"), "w", &world(WORLD), saves.path())
            .unwrap()
            .unwrap();
        let mut bench = BenchDescribe {
            name: "b".into(),
            version: "1.0".into(),
            world: vec![
                WorldPart {
                    part: "desk".into(),
                    integration: "i".into(),
                    config: json!({"domain": "x"}),
                },
                WorldPart {
                    part: "db".into(),
                    integration: "j".into(),
                    config: Value::Null,
                },
            ],
            ..Default::default()
        };
        apply(&mut bench, &data).unwrap();
        assert!(bench.version.starts_with("1.0-data-"));
        assert_eq!(bench.world[0].config["domain"], "x");
        assert_eq!(bench.world[1].config[KEY], data);
        assert_eq!(of_world(&bench.world), Some(&data));
        let fp = fingerprint(json!({"v": 1}), &bench.world[0].config);
        assert_eq!(fp[KEY], data["digest"]);
        assert_eq!(fingerprint(json!({"v": 1}), &json!({})), json!({"v": 1}));
    }
}
