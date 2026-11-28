//! A project kept partly in database tables. Each home chooses which parts are rows
//! ([`TableKinds`]): the settings, and any of scenarios, setups and experiments; worlds are always
//! files. A [`Snapshot`] is what the home read (the rows and the file [`Store`]), and every change
//! to a row kind is collected as a typed [`ChangeSet`] for the home to apply in one transaction,
//! while file kinds are still written to the store.
//!
//! ```text
//! Snapshot ─► Project::open_snapshot ─► put / delete / set_config / undo ─► take_changes
//!   rows + Store                         rows: ChangeSet, files: Store       └─► ChangeSet
//! ```

use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use crate::format::{Kind, ProjectFile};
use crate::store::Store;
use crate::Id;

/// Which parts of a project a home keeps as rows. Worlds are always files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TableKinds {
    /// The project's settings (`shakedown.yaml`).
    pub settings: bool,
    pub scenarios: bool,
    pub setups: bool,
    pub experiments: bool,
}

impl TableKinds {
    /// Everything in files: a project folder as it always was.
    pub const NONE: TableKinds = TableKinds {
        settings: false,
        scenarios: false,
        setups: false,
        experiments: false,
    };
    /// A local project: settings and experiments are rows; worlds, scenarios and setups files.
    pub const LOCAL: TableKinds = TableKinds {
        settings: true,
        experiments: true,
        ..TableKinds::NONE
    };
    /// A hosted project: everything but worlds is rows.
    pub const HOSTED: TableKinds = TableKinds {
        settings: true,
        scenarios: true,
        setups: true,
        experiments: true,
    };

    /// Whether items of `kind` are rows.
    pub fn has(self, kind: Kind) -> bool {
        match kind {
            Kind::World => false,
            Kind::Scenario => self.scenarios,
            Kind::Setup => self.setups,
            Kind::Experiment => self.experiments,
        }
    }

    /// Whether nothing is a row.
    pub fn is_none(self) -> bool {
        self == TableKinds::NONE
    }
}

/// The row a write is based on: its id and the revision the writer read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Base {
    pub id: Uuid,
    pub revision: i64,
}

/// An item as its row holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub kind: Kind,
    pub id: Uuid,
    pub slug: String,
    /// The item exactly as its file would say, `format:` included.
    pub body: Value,
    pub revision: i64,
}

/// What a home read: the settings when they are a row, every live row of its table kinds, and
/// the store that holds its files (the worlds, and every kind that is not a row).
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Revision of the project row read with the snapshot.
    pub revision: i64,
    pub tables: TableKinds,
    /// The settings, when `tables.settings`.
    pub settings: Option<ProjectFile>,
    pub rows: Vec<Row>,
    pub store: Arc<dyn Store>,
}

/// One row a change writes: a new row when `base` is None, a delete when `after` is None.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemChange {
    /// Restore the soft-deleted base row rather than create a new identity.
    pub restore: bool,
    pub kind: Kind,
    pub slug: Id,
    pub base: Option<Base>,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// One file a change wrote or removed, with its bytes before and after, for the history.
#[derive(Debug, Clone, PartialEq)]
pub struct FileChange {
    pub path: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}

/// The settings a change replaces: refused when they are no longer `before`.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsChange {
    pub base_revision: i64,
    pub before: ProjectFile,
    pub after: ProjectFile,
}

/// One change to a project with table kinds: its rows to write, in order, the files it already
/// wrote to the store, and the settings. Applied in one transaction and kept as one entry of the
/// history (under `id`); `undoes` names the change an undo reverses.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeSet {
    pub id: Uuid,
    /// What it did, e.g. `delete world mill`.
    pub label: String,
    pub settings: Option<SettingsChange>,
    pub items: Vec<ItemChange>,
    pub files: Vec<FileChange>,
    pub undoes: Option<Uuid>,
}

impl ChangeSet {
    /// An empty change labelled `label`.
    pub fn new(label: String) -> ChangeSet {
        ChangeSet {
            id: Uuid::new_v4(),
            label,
            settings: None,
            items: Vec::new(),
            files: Vec::new(),
            undoes: None,
        }
    }

    /// Whether it changes nothing.
    pub fn is_empty(&self) -> bool {
        self.settings.is_none() && self.items.is_empty() && self.files.is_empty()
    }
}
