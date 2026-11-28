//! Where a project's files live. Everything above this module (formats, loading, references,
//! deletes, the trash) reads and writes through [`Store`], by paths relative to the project:
//! [`FsStore`] is a folder on disk (personal use), [`MemStore`] an in-memory copy whose changes a
//! caller collects and saves elsewhere in one go (hosted Shakedown keeps them in Postgres), and
//! [`Overlay`] holds a change's writes over another store until the caller flushes them.

use std::io;
use std::path::Path;

mod fs;
mod mem;
mod overlay;

pub use fs::FsStore;
pub use mem::{Change, Files, MemStore};
pub use overlay::Overlay;

/// The longest path a store takes, in bytes.
pub const MAX_PATH: usize = 1024;

/// A file or folder in a store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its last path segment.
    pub name: String,
    pub dir: bool,
    /// A link to somewhere else (only a folder on disk has them).
    pub link: bool,
    /// The file's size in bytes (0 for a folder).
    pub size: u64,
}

/// A project's files, by path relative to the project (`worlds/mill/world.yaml`). Every method
/// refuses a path [`check_path`] refuses. A write replaces the whole file at once: a reader sees
/// the old contents or the new, never part of either.
pub trait Store: std::fmt::Debug + Send + Sync {
    /// The file at `path`.
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
    /// At most `max` bytes from the start of file `file` in folder `dir`, and its full size;
    /// InvalidInput when the file is outside `dir` once links are followed.
    fn read_head(&self, dir: &str, file: &str, max: u64) -> io::Result<(Vec<u8>, u64)> {
        let mut bytes = self.read(&format!("{dir}/{file}"))?;
        let size = bytes.len() as u64;
        bytes.truncate(usize::try_from(max).unwrap_or(usize::MAX));
        Ok((bytes, size))
    }
    /// The entries directly in folder `dir` (`""`: the project's top), in no set order; NotFound
    /// when there is no such folder.
    fn list(&self, dir: &str) -> io::Result<Vec<Entry>>;
    /// The file or folder at `path`, if there is one.
    fn stat(&self, path: &str) -> Option<Entry>;
    /// Creates or replaces the file at `path`, with its folders.
    fn write(&self, path: &str, bytes: &[u8]) -> io::Result<()>;
    /// Deletes the file at `path`.
    fn remove(&self, path: &str) -> io::Result<()>;
    /// Moves the file at `from` to `to`, replacing any file there.
    fn rename(&self, from: &str, to: &str) -> io::Result<()>;
    /// Deletes folder `dir` and everything in it.
    fn remove_dir(&self, dir: &str) -> io::Result<()>;
    /// Makes folder `dir` (a store with no empty folders needs do nothing).
    fn make_dir(&self, _dir: &str) -> io::Result<()> {
        Ok(())
    }
    /// The folder on disk the files are in, for a store that has one.
    fn folder(&self) -> Option<&Path> {
        None
    }

    /// Whether there is a file or folder at `path`.
    fn exists(&self, path: &str) -> bool {
        self.stat(path).is_some()
    }

    fn is_dir(&self, path: &str) -> bool {
        self.stat(path).is_some_and(|e| e.dir)
    }

    fn is_file(&self, path: &str) -> bool {
        self.stat(path).is_some_and(|e| !e.dir)
    }

    /// The file at `path` as text.
    fn read_text(&self, path: &str) -> io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// Refuses a path that could leave the project: empty, absolute, longer than [`MAX_PATH`], with
/// an empty, `.` or `..` segment, a backslash or a NUL.
pub fn check_path(path: &str) -> io::Result<()> {
    let bad = |s: &str| s.is_empty() || s == "." || s == "..";
    let ok = !path.is_empty()
        && path.len() <= MAX_PATH
        && !path.contains(['\\', '\0'])
        && !path.split('/').any(bad);
    if ok {
        Ok(())
    } else {
        let why = format!("{path:?} is not a path inside the project");
        Err(io::Error::new(io::ErrorKind::InvalidInput, why))
    }
}

/// A `.yaml`, `.yml` or `.json` file's contents as JSON, for a store that indexes them; `None`
/// for any other file, one that does not parse, or one holding a NUL character (which some
/// databases' JSON cannot hold).
pub fn parsed(path: &str, bytes: &[u8]) -> Option<serde_json::Value> {
    let value: serde_json::Value = match path.rsplit_once('.')?.1 {
        "yaml" | "yml" => serde_yaml_ng::from_slice(bytes).ok()?,
        "json" => serde_json::from_slice(bytes).ok()?,
        _ => return None,
    };
    let text = value.to_string();
    (!text.contains("\\u0000")).then_some(value)
}

/// Like [`check_path`], allowing `""` for the project's top folder.
pub(crate) fn check_dir(dir: &str) -> io::Result<()> {
    if dir.is_empty() {
        Ok(())
    } else {
        check_path(dir)
    }
}

#[cfg(test)]
mod tests;
