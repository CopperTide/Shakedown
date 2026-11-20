//! `inspect` and `diff`: a summary and a content hash per volume, the probes' docs and the image
//! each service runs.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{json, Map, Value};
use shakedown_part::helpers::folder::Digest;
use shakedown_part::helpers::tar_digest;
use shakedown_part::helpers::temp_dir::TempDir;
use shakedown_part::proto::Docs;
use shakedown_part::PartError;

use crate::volumes::{self, Manifest};

/// The key a volume has in `inspect` and `diff`.
pub fn volume_key(volume: &str) -> String {
    format!("volume/{volume}")
}

/// The digest of every volume in `root` (a save, or a live export), by key: archives
/// (`<volume>.tar`), database dumps (`<volume>.sql`, comments left out) and, in a save, the
/// hard-linked trees its manifest names. Skipped volumes have none.
pub fn digests(root: &Path) -> Result<DigestMap, PartError> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(root).map_err(PartError::internal_from)? {
        let path = entry.map_err(PartError::internal_from)?.path();
        if let Some((volume, d)) = file_digest(&path)? {
            out.insert(volume_key(&volume), d);
        }
    }
    if let Some(m) = Manifest::read(root) {
        add_trees(root, &m, &mut out)?;
    }
    Ok(out)
}

/// The volume and digest of an archive or a dump; None for any other file.
fn file_digest(path: &Path) -> Result<Option<(String, Digest)>, PartError> {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(None);
    };
    if let Some(volume) = name.strip_suffix(".tar") {
        let d = tar_digest::digest(path).map_err(PartError::internal_from)?;
        return Ok(Some((volume.to_string(), d)));
    }
    let Some(volume) = name.strip_suffix(".sql") else {
        return Ok(None);
    };
    let dump = std::fs::read_to_string(path).map_err(PartError::internal_from)?;
    Ok(Some((volume.to_string(), volumes::dump_digest(&dump))))
}

/// The digests of the save's trees, each archived by the helper as a live volume is.
fn add_trees(root: &Path, m: &Manifest, out: &mut DigestMap) -> Result<(), PartError> {
    let trees: Vec<&str> = (m.volumes.iter())
        .filter(|v| m.has_tree(&v.name) && !out.contains_key(&volume_key(&v.name)))
        .map(|v| v.name.as_str())
        .collect();
    for name in trees {
        let tmp = TempDir::new_in(&std::env::temp_dir(), "tree");
        let tmp = tmp.map_err(PartError::internal_from)?;
        let tar = tmp.path().join("tree.tar");
        volumes::archive_tree(root, name, &tar).map_err(PartError::internal_from)?;
        let d = tar_digest::digest(&tar).map_err(PartError::internal_from)?;
        out.insert(volume_key(name), d);
    }
    Ok(())
}

/// A target's volumes by key.
pub type DigestMap = BTreeMap<String, Digest>;

fn doc(d: &Digest) -> Value {
    json!({"bytes": d.bytes, "files": d.files, "hash": d.hash})
}

/// Each volume's doc `{bytes, files, hash}`, by key.
pub fn volume_docs(digests: &DigestMap) -> Map<String, Value> {
    (digests.iter()).map(|(k, d)| (k.clone(), doc(d))).collect()
}

/// `image/<service>` → `{image}` for each service's image.
pub fn image_docs(images: &BTreeMap<String, String>) -> Map<String, Value> {
    let doc = |(s, i): (&String, &String)| (format!("image/{s}"), json!({"image": i}));
    images.iter().map(doc).collect()
}

/// `keys` of `all` (every doc when empty); a key it lacks is null.
pub fn select(mut all: Map<String, Value>, keys: &[String]) -> Docs {
    if keys.is_empty() {
        return Docs { docs: all };
    }
    let docs = (keys.iter())
        .map(|k| (k.clone(), all.remove(k).unwrap_or(Value::Null)))
        .collect();
    Docs { docs }
}

/// The volume keys whose content differs between two sets of digests.
pub fn diff(a: &BTreeMap<String, Digest>, b: &BTreeMap<String, Digest>) -> Vec<String> {
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    keys.into_iter()
        .filter(|k| a.get(*k).map(|d| &d.hash) != b.get(*k).map(|d| &d.hash))
        .cloned()
        .collect()
}
