//! The trash: each delete moves its files to `.shakedown/trash/<stamp>/` at their paths in the
//! project, with copies of the files it rewrote, and `manifest.json` naming both (with a hash of
//! what the delete wrote, so an undo never overwrites a later edit). Restoring a delete puts them
//! all back. `.shakedown/` holds a `.gitignore` of `*`, so no project commits its trash, and the
//! deletes of the last 30 days are kept, and always the last 20.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::ProjectError;
use crate::load::{hash, io};
use crate::project::write;
use crate::store::Store;
use crate::Project;

const TRASH: &str = ".shakedown/trash";
const MANIFEST: &str = "manifest.json";
/// The deletes kept whatever their age.
const KEEP_AT_LEAST: usize = 20;
/// How long a delete is kept beyond the last [`KEEP_AT_LEAST`].
const KEEP_MILLIS: u128 = 30 * 24 * 60 * 60 * 1000;

/// What one delete did: what was deleted (`world mill`), the files it removed and the files it
/// rewrote, as paths in the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub label: String,
    pub deleted: Vec<String>,
    pub edited: Vec<Edited>,
}

/// A file a delete rewrote, and the SHA-256 of what it wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edited {
    pub file: String,
    pub sha256: String,
}

impl Manifest {
    pub(crate) fn new(label: String, deleted: &[String], edits: &[(String, String)]) -> Manifest {
        let edited = edits.iter().map(|(file, yaml)| Edited {
            file: file.clone(),
            sha256: hash(yaml.as_bytes()),
        });
        Manifest {
            label,
            deleted: deleted.to_vec(),
            edited: edited.collect(),
        }
    }

    /// Every file the delete removed or rewrote.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        let edited = self.edited.iter().map(|e| e.file.as_str());
        self.deleted.iter().map(String::as_str).chain(edited)
    }
}

/// One delete's trash folder.
pub(crate) struct Bin {
    store: Arc<dyn Store>,
    /// `.shakedown/trash/<name>`.
    dir: String,
    /// The folder's name, which names the delete.
    pub(crate) name: String,
}

impl Bin {
    /// A new trash folder for a delete, holding its manifest before any file moves; older
    /// deletes past what is kept go.
    pub(crate) fn open(store: Arc<dyn Store>, manifest: &Manifest) -> Result<Bin, ProjectError> {
        if !store.exists(".shakedown/.gitignore") {
            write(&*store, ".shakedown/.gitignore", "*\n")?;
        }
        let now = now_millis();
        prune(&*store, now);
        let stamp = format!("{now:013}");
        let mut name = stamp.clone();
        for n in 1.. {
            if !store.exists(&format!("{TRASH}/{name}")) {
                break;
            }
            name = format!("{stamp}-{n}");
        }
        let json = serde_json::to_string_pretty(manifest).unwrap_or_default();
        write(&*store, &format!("{TRASH}/{name}/{MANIFEST}"), &json)?;
        let dir = format!("{TRASH}/{name}");
        Ok(Bin { store, dir, name })
    }

    /// Keeps a copy of `file` as it is now, before it is rewritten.
    pub(crate) fn keep_copy(&self, file: &str) -> Result<(), ProjectError> {
        let to = format!("{}/{file}", self.dir);
        let copy = self
            .store
            .read(file)
            .and_then(|b| self.store.write(&to, &b));
        copy.map_err(|e| io("copying to the trash", file, e))
    }

    /// Moves `file` into the trash.
    pub(crate) fn take(&self, file: &str) -> Result<(), ProjectError> {
        let to = format!("{}/{file}", self.dir);
        (self.store.rename(file, &to)).map_err(|e| io("moving to the trash", file, e))
    }
}

impl Project {
    /// The delete named `name` (the most recent when `None`): its trash folder and manifest.
    pub fn trashed(&self, name: Option<&str>) -> Result<(String, Manifest), ProjectError> {
        let name = match name {
            Some(n) if is_stamp(n) => n.to_string(),
            Some(n) => return Err(no_delete(n)),
            None => latest(&**self.store()).ok_or_else(|| no_delete("recent"))?,
        };
        let file = format!("{TRASH}/{name}/{MANIFEST}");
        let text = self
            .store()
            .read_text(&file)
            .map_err(|_| no_delete(&name))?;
        let manifest = serde_json::from_str(&text).map_err(|_| no_delete(&name))?;
        Ok((name, manifest))
    }

    /// Undoes the delete named `name` (the most recent when `None`): every file it removed or
    /// rewrote is put back and its trash folder is emptied. Refused while a removed file's path
    /// is taken again, or a rewritten file was edited since. Answers what was put back.
    pub fn restore(&mut self, name: Option<&str>) -> Result<Manifest, ProjectError> {
        let (name, manifest) = self.trashed(name)?;
        let dir = format!("{TRASH}/{name}");
        self.check_restore(&dir, &manifest)?;
        let store = self.store().clone();
        for file in manifest.files() {
            let from = format!("{dir}/{file}");
            if store.exists(&from) {
                store
                    .rename(&from, file)
                    .map_err(|e| io("restoring", file, e))?;
            }
        }
        (store.remove_dir(&dir)).map_err(|e| io("emptying the trash of", &name, e))?;
        self.reload()?;
        Ok(manifest)
    }

    /// Refuses an undo that would overwrite a file: a removed file's path taken again, or a
    /// rewritten file that no longer holds what the delete wrote (a missing one is put back).
    fn check_restore(&self, dir: &str, manifest: &Manifest) -> Result<(), ProjectError> {
        let store = &**self.store();
        let kept = |f: &str| store.exists(&format!("{dir}/{f}"));
        let taken = |f: &&String| kept(f) && store.exists(f);
        if let Some(file) = manifest.deleted.iter().find(taken) {
            return Err(ProjectError::Occupied { file: file.clone() });
        }
        let changed = |e: &&Edited| {
            let now = store.read(&e.file);
            kept(&e.file) && now.is_ok_and(|bytes| hash(&bytes) != e.sha256)
        };
        let files: Vec<String> = manifest
            .edited
            .iter()
            .filter(changed)
            .map(|e| e.file.clone())
            .collect();
        if !files.is_empty() {
            return Err(ProjectError::EditedSince { files });
        }
        Ok(())
    }

    /// Keeps `text` as `note` (a plain file name) in the trash of the delete named `name`.
    pub fn write_trash_note(&self, name: &str, note: &str, text: &str) -> Result<(), ProjectError> {
        let file = note_file(name, note)?;
        write(&**self.store(), &file, text)
    }

    /// The `note` kept in the trash of the delete named `name`, if any.
    pub fn trash_note(&self, name: &str, note: &str) -> Option<String> {
        let file = note_file(name, note).ok()?;
        self.store().read_text(&file).ok()
    }
}

/// The path of `note` in the trash of delete `name`.
fn note_file(name: &str, note: &str) -> Result<String, ProjectError> {
    let plain = !note.is_empty()
        && note != MANIFEST
        && note
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        && !note.starts_with('.');
    if !is_stamp(name) || !plain {
        return Err(no_delete(name));
    }
    Ok(format!("{TRASH}/{name}/{note}"))
}

fn now_millis() -> u128 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH);
    since.map_or(0, |d| d.as_millis())
}

/// Removes the deletes older than what is kept: beyond the newest [`KEEP_AT_LEAST`], those
/// older than [`KEEP_MILLIS`]. A folder that can't be removed stays.
fn prune(store: &dyn Store, now: u128) {
    let mut names = stamps(store);
    names.reverse();
    for (millis, name) in names.into_iter().skip(KEEP_AT_LEAST) {
        if millis + KEEP_MILLIS < now {
            let _ = store.remove_dir(&format!("{TRASH}/{name}"));
        }
    }
}

/// The trash folders, oldest first, each with the time it was made.
fn stamps(store: &dyn Store) -> Vec<(u128, String)> {
    let Ok(entries) = store.list(TRASH) else {
        return Vec::new();
    };
    let names = entries.into_iter().map(|e| e.name);
    let mut found: Vec<(u128, u32, String)> = names
        .filter(|n| is_stamp(n))
        .map(|n| {
            let (stamp, k) = n.split_once('-').unwrap_or((&n, "0"));
            (
                stamp.parse().unwrap_or(0),
                k.parse().unwrap_or(0),
                n.clone(),
            )
        })
        .collect();
    found.sort();
    found
        .into_iter()
        .map(|(millis, _, name)| (millis, name))
        .collect()
}

/// A trash folder's name: digits, then maybe `-` and digits.
fn is_stamp(name: &str) -> bool {
    let (stamp, n) = name.split_once('-').unwrap_or((name, "0"));
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    digits(stamp) && digits(n)
}

/// The newest trash folder's name.
fn latest(store: &dyn Store) -> Option<String> {
    stamps(store).pop().map(|(_, name)| name)
}

fn no_delete(name: &str) -> ProjectError {
    ProjectError::NotFound {
        kind: "delete to undo",
        id: name.to_string(),
    }
}
