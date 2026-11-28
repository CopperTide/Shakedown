//! The kinds a project keeps as rows: opening a project from a [`Snapshot`], and the writes that
//! go to rows (collected in the pending [`ChangeSet`]) or to files (written to the store, and
//! noted in the change so the history can undo them). A project with no table kinds writes files
//! exactly as it always did and collects nothing.

use std::collections::BTreeMap;

use serde_json::Value;
use uuid::Uuid;

use crate::error::ProjectError;
use crate::format::{Kind, ProjectFile};
use crate::item::{kind_name, Item, ItemBody, Problem};
use crate::load::{io, item_file, parse_item, Items};
use crate::project::Project;
use crate::tables::{
    Base, ChangeSet, FileChange, ItemChange, Row, SettingsChange, Snapshot, TableKinds,
};
use crate::Id;

/// The rows of a project with table kinds, and the change made to them since it was opened.
#[derive(Debug, Clone)]
pub(crate) struct Db {
    pub(crate) revision: i64,
    pub(crate) kinds: TableKinds,
    /// Each live row by kind and slug: the row it is (None: made by the pending change) and its
    /// body now.
    pub(crate) rows: BTreeMap<(Kind, Id), RowNow>,
    /// Rows whose slug is not an id: listed as problems, kept for export.
    pub(crate) bad: Vec<Row>,
    pub(crate) pending: Option<ChangeSet>,
}

/// A row as the project holds it now.
#[derive(Debug, Clone)]
pub(crate) struct RowNow {
    pub(crate) base: Option<Base>,
    pub(crate) body: Value,
}

impl Db {
    fn insert(&mut self, row: Row) {
        let Ok(id) = Id::parse(&row.slug) else {
            self.bad.push(row);
            return;
        };
        let base = Some(Base {
            id: row.id,
            revision: row.revision,
        });
        self.rows.insert(
            (row.kind, id),
            RowNow {
                base,
                body: row.body,
            },
        );
    }

    /// Adds every row to `items`, or the problem that stops it.
    pub(crate) fn load_into(&self, items: &mut Items, problems: &mut Vec<Problem>) {
        for ((kind, id), row) in &self.rows {
            match row_item(*kind, id, &row.body) {
                Ok(item) => {
                    items.insert((*kind, id.clone()), item);
                }
                Err(p) => problems.push(p),
            }
        }
        problems.extend(self.bad.iter().map(|r| Problem {
            kind: Some(r.kind),
            id: Some(r.slug.clone()),
            pointer: String::new(),
            message: ProjectError::InvalidId(r.slug.clone()).to_string(),
        }));
    }
}

/// The item a row holds, read exactly as its file would be.
fn row_item(kind: Kind, id: &Id, body: &Value) -> Result<Item, Problem> {
    let text = serde_yaml_ng::to_string(body).unwrap_or_default();
    let item = parse_item(kind, id, text.as_bytes())?;
    let canonical = item.body.to_yaml().unwrap_or(text);
    parse_item(kind, id, canonical.as_bytes())
}

impl Project {
    /// Opens the project a home read: its rows and its files. Fails only when the settings are
    /// missing or unreadable; every other problem is listed in `problems`.
    pub fn open_snapshot(s: Snapshot) -> Result<Project, ProjectError> {
        if s.tables.is_none() {
            return Project::open_in(s.store);
        }
        let config = match (s.tables.settings, s.settings) {
            (true, Some(config)) => config,
            (true, None) => return Err(ProjectError::NotAProject),
            (false, _) => Project::open_in(s.store.clone())?.config,
        };
        let mut db = Db {
            revision: s.revision,
            kinds: s.tables,
            rows: BTreeMap::new(),
            bad: Vec::new(),
            pending: None,
        };
        for row in s.rows.into_iter().filter(|r| s.tables.has(r.kind)) {
            db.insert(row);
        }
        Project::assemble(s.store, config, Some(Box::new(db)))
    }

    /// The kinds this project keeps as rows.
    pub fn tables(&self) -> TableKinds {
        self.db.as_ref().map_or(TableKinds::NONE, |d| d.kinds)
    }

    /// The project row revision read with this snapshot.
    pub fn project_revision(&self) -> Option<i64> {
        self.db.as_ref().map(|db| db.revision)
    }

    /// The revision of item `id`'s row, for a kind kept as rows.
    pub fn revision(&self, kind: Kind, id: &Id) -> Option<i64> {
        self.row_base(kind, id).map(|b| b.revision)
    }

    /// The id of item `id`'s row, for a kind kept as rows.
    pub fn row_id(&self, kind: Kind, id: &Id) -> Option<Uuid> {
        self.row_base(kind, id).map(|b| b.id)
    }

    /// Whether a live row of `kind` has slug `id`.
    pub(crate) fn has_row(&self, kind: Kind, id: &Id) -> bool {
        let db = self.db.as_ref();
        db.is_some_and(|d| d.rows.contains_key(&(kind, id.clone())))
    }

    /// The revision of the live row with id `row`.
    pub(crate) fn row_revision(&self, row: Option<Uuid>) -> Option<i64> {
        let db = self.db.as_ref()?;
        let mut bases = db.rows.values().filter_map(|r| r.base);
        bases.find(|b| Some(b.id) == row).map(|b| b.revision)
    }

    fn row_base(&self, kind: Kind, id: &Id) -> Option<Base> {
        let db = self.db.as_ref()?;
        db.rows.get(&(kind, id.clone()))?.base
    }

    /// The change made since the project was opened (or last taken), for the home to apply;
    /// None when nothing changed or no kind is a row.
    pub fn take_changes(&mut self) -> Option<ChangeSet> {
        let db = self.db.as_mut()?;
        db.pending.take().filter(|c| !c.is_empty())
    }

    /// The id the pending change will be kept under, made now if there is none.
    pub(crate) fn change_id(&mut self, label: &str) -> Option<Uuid> {
        self.pending(label).map(|c| c.id)
    }

    /// The pending change, begun with `label` if there is none; None with no table kinds.
    pub(crate) fn pending(&mut self, label: &str) -> Option<&mut ChangeSet> {
        let db = self.db.as_mut()?;
        Some(
            db.pending
                .get_or_insert_with(|| ChangeSet::new(label.to_string())),
        )
    }

    /// Writes item `id` of `kind`: its row when `kind` is a row kind, else its file.
    pub(crate) fn store_item(
        &mut self,
        label: &str,
        kind: Kind,
        id: &Id,
        body: &ItemBody,
        yaml: &str,
        base_revision: Option<i64>,
    ) -> Result<(), ProjectError> {
        if !self.tables().has(kind) {
            return self.write_file(label, &item_file(kind, id), yaml.as_bytes());
        }
        let after = serde_json::to_value(body)
            .map_err(|e| crate::FormatError::new(&item_file(kind, id), "", e.to_string()))?;
        self.write_row(label, kind, id, Some(after), base_revision)
    }

    /// Deletes item `id` of `kind`: its row, or its file.
    pub(crate) fn drop_item(
        &mut self,
        label: &str,
        kind: Kind,
        id: &Id,
    ) -> Result<(), ProjectError> {
        if self.tables().has(kind) {
            return self.write_row(label, kind, id, None, None);
        }
        self.remove_file(label, &item_file(kind, id))
    }

    /// Sets row `id` of `kind` to `after` (None: deletes it) in the pending change, refused when
    /// `base_revision` is given and the row is not at it.
    pub(crate) fn write_row(
        &mut self,
        label: &str,
        kind: Kind,
        id: &Id,
        after: Option<Value>,
        base_revision: Option<i64>,
    ) -> Result<(), ProjectError> {
        let key = (kind, id.clone());
        let now = self.db.as_ref().and_then(|d| d.rows.get(&key)).cloned();
        let current = now.as_ref().and_then(|r| r.base).map(|b| b.revision);
        if let Some(based_on) = base_revision.filter(|b| Some(*b) != current) {
            let (kind, id) = (kind_name(kind), id.to_string());
            return Err(ProjectError::Changed {
                kind,
                id,
                based_on,
                current,
            });
        }
        let set = self
            .pending(label)
            .expect("a row kind is kept by a project with table kinds");
        let base = merge_item(set, kind, id, now.as_ref(), after.clone());
        let db = self
            .db
            .as_mut()
            .expect("only a project with table kinds has a pending change");
        match after {
            Some(body) => db.rows.insert(key, RowNow { base, body }),
            None => db.rows.remove(&key),
        };
        Ok(())
    }

    /// Replaces the settings in the pending change.
    pub(crate) fn change_settings(&mut self, after: ProjectFile) {
        let before = self.config.clone();
        let base_revision = self.db.as_ref().map_or(0, |db| db.revision);
        if let Some(set) = self.pending("edit settings") {
            let before = set.settings.take().map_or(before, |s| s.before);
            set.settings = Some(SettingsChange {
                base_revision,
                before,
                after: after.clone(),
            });
        }
        self.config = after;
    }

    /// Replaces file `path` with `contents`, noting it in the pending change.
    pub(crate) fn write_file(
        &mut self,
        label: &str,
        path: &str,
        contents: &[u8],
    ) -> Result<(), ProjectError> {
        let before = self.noted_before(path);
        self.store
            .write(path, contents)
            .map_err(|e| io("writing", path, e))?;
        self.note_file(label, path, before, Some(contents.to_vec()));
        Ok(())
    }

    /// Deletes file `path`, noting it in the pending change.
    pub(crate) fn remove_file(&mut self, label: &str, path: &str) -> Result<(), ProjectError> {
        let before = self.noted_before(path);
        self.store
            .remove(path)
            .map_err(|e| io("deleting", path, e))?;
        self.note_file(label, path, before, None);
        Ok(())
    }

    /// File `path`'s bytes now, when a change notes files (with table kinds).
    fn noted_before(&self, path: &str) -> Option<Vec<u8>> {
        self.db.as_ref()?;
        self.store.read(path).ok()
    }

    fn note_file(
        &mut self,
        label: &str,
        path: &str,
        before: Option<Vec<u8>>,
        after: Option<Vec<u8>>,
    ) {
        let Some(set) = self.pending(label) else {
            return;
        };
        match set.files.iter_mut().find(|f| f.path == path) {
            Some(f) => f.after = after,
            None => set.files.push(FileChange {
                path: path.to_string(),
                before,
                after,
            }),
        }
        set.files
            .retain(|f| f.before.is_some() || f.after.is_some());
    }
}

/// Adds the write of row `id` to `set`, folding it into an earlier write of the same row, and
/// answers the row it writes (None: a new one). A row made and deleted again in one change is no
/// write at all.
fn merge_item(
    set: &mut ChangeSet,
    kind: Kind,
    id: &Id,
    now: Option<&RowNow>,
    after: Option<Value>,
) -> Option<Base> {
    let base = match set
        .items
        .iter_mut()
        .find(|c| c.kind == kind && c.slug == *id)
    {
        Some(c) => {
            c.after = after;
            c.base
        }
        None => {
            let base = now.and_then(|r| r.base);
            set.items.push(ItemChange {
                restore: false,
                kind,
                slug: id.clone(),
                base,
                before: now.map(|r| r.body.clone()),
                after,
            });
            base
        }
    };
    set.items.retain(|c| c.base.is_some() || c.after.is_some());
    base
}
