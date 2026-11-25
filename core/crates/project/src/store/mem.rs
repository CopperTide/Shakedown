//! A project held in memory: loaded from wherever its files are kept, changed like a folder, its
//! changes collected with [`MemStore::changes`] for the caller to save in one go. Folders are
//! implied by the files in them, so an empty folder does not exist.

use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

use super::{check_dir, check_path, Entry, Store};

/// Files by path.
pub type Files = BTreeMap<String, Arc<[u8]>>;

/// One file's change: its new contents, or `None` when it was removed.
pub type Change = (String, Option<Arc<[u8]>>);

/// A project in memory.
#[derive(Debug, Default)]
pub struct MemStore {
    state: Mutex<State>,
    /// Refuses every write: a copy to read, whose changes would be lost.
    read_only: bool,
}

#[derive(Debug, Default)]
struct State {
    files: Files,
    /// Each path changed since the start, with what it holds now.
    changed: BTreeMap<String, Option<Arc<[u8]>>>,
}

impl MemStore {
    /// A store holding `files`, with no changes yet.
    pub fn new(files: Files) -> MemStore {
        let state = State {
            files,
            changed: BTreeMap::new(),
        };
        MemStore {
            state: Mutex::new(state),
            read_only: false,
        }
    }

    /// A store holding `files` that refuses every write.
    pub fn read_only(files: Files) -> MemStore {
        MemStore {
            read_only: true,
            ..MemStore::new(files)
        }
    }

    /// The state, to change; refused when read-only.
    fn change(&self) -> io::Result<MutexGuard<'_, State>> {
        if self.read_only {
            let why = "this copy of the project is read-only";
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, why));
        }
        Ok(self.lock())
    }

    /// Every file changed since the start, in path order, with its new contents (`None`:
    /// removed). A file written then removed again is a removal.
    pub fn changes(&self) -> Vec<Change> {
        let state = self.lock();
        state.changed.clone().into_iter().collect()
    }

    /// The files as they are now.
    pub fn files(&self) -> Files {
        self.lock().files.clone()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl State {
    fn put(&mut self, path: &str, bytes: Option<Arc<[u8]>>) {
        match &bytes {
            Some(b) => self.files.insert(path.to_string(), b.clone()),
            None => self.files.remove(path),
        };
        self.changed.insert(path.to_string(), bytes);
    }

    /// The files under folder `dir` (`""`: all).
    fn under<'a>(&'a self, dir: &str) -> impl Iterator<Item = (&'a String, &'a Arc<[u8]>)> {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let range = self.files.range(prefix.clone()..);
        range.take_while(move |(p, _)| p.starts_with(&prefix))
    }

    fn is_dir(&self, path: &str) -> bool {
        self.under(path).next().is_some()
    }

    /// Refuses a write at `path` where a folder is, or below a file.
    fn writable(&self, path: &str) -> io::Result<()> {
        let mut parents = path.match_indices('/').map(|(i, _)| &path[..i]);
        if self.is_dir(path) || parents.any(|p| self.files.contains_key(p)) {
            let why = format!("{path:?} is in the way of a file or folder");
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, why));
        }
        Ok(())
    }
}

fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("no {path:?}"))
}

impl Store for MemStore {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        check_path(path)?;
        let state = self.lock();
        let bytes = state.files.get(path).ok_or_else(|| not_found(path))?;
        Ok(bytes.to_vec())
    }

    fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        check_dir(dir)?;
        let state = self.lock();
        let skip = if dir.is_empty() { 0 } else { dir.len() + 1 };
        let mut found: BTreeMap<&str, Entry> = BTreeMap::new();
        for (path, bytes) in state.under(dir) {
            let rest = &path[skip..];
            let (name, dir) = rest
                .split_once('/')
                .map_or((rest, false), |(n, _)| (n, true));
            let size = if dir { 0 } else { bytes.len() as u64 };
            let entry = Entry {
                name: name.to_string(),
                dir,
                link: false,
                size,
            };
            found.entry(name).or_insert(entry);
        }
        if found.is_empty() && !dir.is_empty() {
            return Err(not_found(dir));
        }
        Ok(found.into_values().collect())
    }

    fn stat(&self, path: &str) -> Option<Entry> {
        check_path(path).ok()?;
        let state = self.lock();
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        if let Some(bytes) = state.files.get(path) {
            let size = bytes.len() as u64;
            return Some(Entry {
                name,
                dir: false,
                link: false,
                size,
            });
        }
        state.is_dir(path).then_some(Entry {
            name,
            dir: true,
            link: false,
            size: 0,
        })
    }

    fn write(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        check_path(path)?;
        let mut state = self.change()?;
        state.writable(path)?;
        state.put(path, Some(Arc::from(bytes)));
        Ok(())
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        check_path(path)?;
        let mut state = self.change()?;
        if !state.files.contains_key(path) {
            return Err(not_found(path));
        }
        state.put(path, None);
        Ok(())
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        check_path(from)?;
        check_path(to)?;
        let mut state = self.change()?;
        let bytes = state
            .files
            .get(from)
            .cloned()
            .ok_or_else(|| not_found(from))?;
        if from == to {
            return Ok(());
        }
        state.writable(to)?;
        state.put(from, None);
        state.put(to, Some(bytes));
        Ok(())
    }

    fn remove_dir(&self, dir: &str) -> io::Result<()> {
        check_path(dir)?;
        let mut state = self.change()?;
        let doomed: Vec<String> = state.under(dir).map(|(p, _)| p.clone()).collect();
        if doomed.is_empty() {
            return Err(not_found(dir));
        }
        for path in doomed {
            state.put(&path, None);
        }
        Ok(())
    }
}
