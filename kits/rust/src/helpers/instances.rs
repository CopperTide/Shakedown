//! Instance bookkeeping: records of running instances on disk (so a restarted program can still
//! save, inspect and stop them), the table of running instances in memory, and a lock per key.
use std::collections::HashMap;
use std::io;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::proto::valid_id;

/// Where an instance's record is.
#[derive(Debug, Clone)]
enum Layout {
    /// `<dir>/<id>.json`.
    Files,
    /// `<dir>/<id>/<file>`, beside whatever else the instance keeps in its folder.
    Folders(&'static str),
}

/// One JSON record per running instance, under `dir` (a dot folder such as `<package saves>/.live`,
/// which arenad's sweep and `gc`'s save removal leave alone).
#[derive(Debug, Clone)]
pub struct LiveRecords<T> {
    dir: PathBuf,
    layout: Layout,
    record: PhantomData<fn() -> T>,
}

impl<T: Serialize + DeserializeOwned> LiveRecords<T> {
    /// Records at `<dir>/<id>.json`.
    pub fn files(dir: PathBuf) -> LiveRecords<T> {
        LiveRecords {
            dir,
            layout: Layout::Files,
            record: PhantomData,
        }
    }

    /// Records at `<dir>/<id>/<file>`.
    pub fn folders(dir: PathBuf, file: &'static str) -> LiveRecords<T> {
        LiveRecords {
            dir,
            layout: Layout::Folders(file),
            record: PhantomData,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The record's file.
    pub fn path(&self, id: &str) -> PathBuf {
        match self.layout {
            Layout::Files => self.dir.join(format!("{id}.json")),
            Layout::Folders(file) => self.dir.join(id).join(file),
        }
    }

    /// Writes `id`'s record (compact JSON), making its folder.
    pub fn record(&self, id: &str, record: &T) -> io::Result<()> {
        let path = self.path(id);
        std::fs::create_dir_all(path.parent().expect("a record has a folder"))?;
        std::fs::write(path, serde_json::to_vec(record)?)
    }

    /// `id`'s record, when it has one that reads.
    pub fn get(&self, id: &str) -> Option<T> {
        serde_json::from_slice(&std::fs::read(self.path(id)).ok()?).ok()
    }

    /// Removes `id`'s record file (nothing when it has none).
    pub fn forget(&self, id: &str) {
        let _ = std::fs::remove_file(self.path(id));
    }

    /// The instances recorded: every `.json` file's name, or every folder named like an id.
    pub fn ids(&self) -> Vec<String> {
        let entries = std::fs::read_dir(&self.dir).into_iter().flatten().flatten();
        let names = entries.filter_map(|e| e.file_name().into_string().ok());
        match self.layout {
            Layout::Files => names
                .filter_map(|n| n.strip_suffix(".json").map(str::to_string))
                .collect(),
            Layout::Folders(_) => names.filter(|n| valid_id(n)).collect(),
        }
    }
}

/// The running instances of this program, by id.
#[derive(Debug)]
pub struct Registry<T> {
    map: Mutex<HashMap<String, T>>,
}

impl<T> Default for Registry<T> {
    fn default() -> Registry<T> {
        Registry {
            map: Mutex::new(HashMap::new()),
        }
    }
}

impl<T> Registry<T> {
    /// The table itself, for reading several instances at once.
    pub fn lock(&self) -> MutexGuard<'_, HashMap<String, T>> {
        self.map.lock().expect("instances lock")
    }

    pub fn insert(&self, id: String, instance: T) {
        self.lock().insert(id, instance);
    }

    pub fn remove(&self, id: &str) -> Option<T> {
        self.lock().remove(id)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.lock().contains_key(id)
    }

    /// Applies `f` to instance `id` when it runs; whether it did.
    pub fn update(&self, id: &str, f: impl FnOnce(&mut T)) -> bool {
        self.lock().get_mut(id).map(f).is_some()
    }
}

impl<T: Clone> Registry<T> {
    /// A copy of instance `id`, when it runs.
    pub fn get(&self, id: &str) -> Option<T> {
        self.lock().get(id).cloned()
    }
}

/// One async lock per key (an instance, a database), made on first use.
#[derive(Debug, Default)]
pub struct KeyedLocks {
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl KeyedLocks {
    /// The lock of `key`.
    pub fn lock_for(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock().expect("keyed locks lock");
        locks.entry(key.to_string()).or_default().clone()
    }

    /// Drops the lock of `key` (a later `lock_for` makes a new one).
    pub fn forget(&self, key: &str) {
        self.locks.lock().expect("keyed locks lock").remove(key);
    }
}
