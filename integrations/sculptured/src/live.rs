//! Running instances on disk: `<package saves>/.live/<instance>/` holds the server's data folder
//! (`data/`), its home (`home/`), its log (`serve.log`), `run.json`, the record a restarted
//! program reads to reach, save and stop the instance, and `aliases.json`, its re-raised changes.
//! A dot folder: shaked's sweep and `gc`'s save removal leave it alone.
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use shakedown_part::helpers::instances::LiveRecords;
use shakedown_part::helpers::remove_tree_or_log;

use crate::aliases::Aliases;
use crate::server::{Folders, Pids};

/// What a running instance is: its config, the program and build it runs, its server's port
/// once it listens, the scratch folder its runner's socket goes in, and its processes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub config: Value,
    pub binary: PathBuf,
    pub build: String,
    pub port: Option<u16>,
    pub tmp: PathBuf,
    #[serde(default)]
    pub pids: Pids,
}

pub struct Live {
    root: PathBuf,
    runs: LiveRecords<Run>,
    aliases: LiveRecords<Aliases>,
}

impl Live {
    pub fn new(root: PathBuf) -> Live {
        Live {
            runs: LiveRecords::folders(root.clone(), "run.json"),
            aliases: LiveRecords::folders(root.clone(), "aliases.json"),
            root,
        }
    }

    fn folder(&self, instance: &str) -> PathBuf {
        self.root.join(instance)
    }

    /// The folders of `instance`'s server, as `run` records them.
    pub fn folders(&self, instance: &str, run: &Run) -> Folders {
        Folders {
            root: self.folder(instance),
            tmp: run.tmp.clone(),
        }
    }

    pub fn record(&self, instance: &str, run: &Run) -> io::Result<()> {
        self.runs.record(instance, run)
    }

    /// The record of `instance`, when it runs (or is starting).
    pub fn run(&self, instance: &str) -> Option<Run> {
        self.runs.get(instance)
    }

    pub fn aliases(&self, instance: &str) -> Aliases {
        self.aliases.get(instance).unwrap_or_default()
    }

    pub fn set_aliases(&self, instance: &str, aliases: &Aliases) -> io::Result<()> {
        self.aliases.record(instance, aliases)
    }

    /// Every instance with a folder here.
    pub fn ids(&self) -> Vec<String> {
        self.runs.ids()
    }

    /// Removes the instance's folder and its scratch folder, if known (failures are logged).
    pub fn remove(&self, instance: &str, tmp: Option<&Path>) {
        remove_tree_or_log(&self.folder(instance));
        if let Some(t) = tmp {
            remove_tree_or_log(t);
        }
    }
}
