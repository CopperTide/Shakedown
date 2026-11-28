//! The history of a project with table kinds, replacing the trash: each applied [`ChangeSet`] is
//! kept as before-and-after records ([`ChangeItem`]), and an undo is planned from them as a new
//! change. Pure: the home reads and writes the records.
//!
//! ```text
//! ChangeSet ──apply──► ChangeSet::records ──► project_changes
//! project_changes ──► Project::undo ──checks──► the inverse ChangeSet (undoes = its id)
//! ```

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::error::ProjectError;
use crate::format::{Kind, ProjectFile};
use crate::load::item_file;
use crate::project::PROJECT_FILE;
use crate::tables::{Base, ChangeSet};
use crate::{FormatError, Id, Project};

/// What a change item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// The project's settings.
    Project,
    Scenario,
    Setup,
    Experiment,
    /// A file of a setup described in MADL.
    SetupFile,
    /// A file of the project folder: a world's files, a scenario or setup kept as a file, or a
    /// note on the change (under [`NOTES`]).
    File,
}

/// Where a change's notes are named: a note is kept as a file item, never put back by an undo.
pub const NOTES: &str = ".shakedown/notes/";

impl ChangeKind {
    const ALL: [ChangeKind; 6] = [
        ChangeKind::Project,
        ChangeKind::Scenario,
        ChangeKind::Setup,
        ChangeKind::Experiment,
        ChangeKind::SetupFile,
        ChangeKind::File,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Project => "project",
            ChangeKind::Scenario => "scenario",
            ChangeKind::Setup => "setup",
            ChangeKind::Experiment => "experiment",
            ChangeKind::SetupFile => "setup_file",
            ChangeKind::File => "file",
        }
    }

    /// The kind named `s`.
    pub fn parse(s: &str) -> Option<ChangeKind> {
        ChangeKind::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// The change kind of a row of `kind` (a world is never a row: [`ChangeKind::File`]).
    pub fn of(kind: Kind) -> ChangeKind {
        match kind {
            Kind::World => ChangeKind::File,
            Kind::Scenario => ChangeKind::Scenario,
            Kind::Setup => ChangeKind::Setup,
            Kind::Experiment => ChangeKind::Experiment,
        }
    }

    /// The item kind of a row's change kind.
    pub fn item_kind(self) -> Option<Kind> {
        match self {
            ChangeKind::Scenario => Some(Kind::Scenario),
            ChangeKind::Setup => Some(Kind::Setup),
            ChangeKind::Experiment => Some(Kind::Experiment),
            _ => None,
        }
    }
}

/// What one change did to one item. None before: it was made; None after: it was deleted. A row
/// (project, scenario, setup, experiment) has `item_id`, its bodies and the revision the change
/// left; a file has its bytes; a setup file has both its setup's id and its bytes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChangeItem {
    pub kind: ChangeKind,
    pub item_id: Option<Uuid>,
    /// The item's slug, or the file's path.
    pub name: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub before_bytes: Option<Vec<u8>>,
    pub after_bytes: Option<Vec<u8>>,
    pub revision: Option<i64>,
}

impl ChangeItem {
    /// A row's change: `kind` is not a file kind.
    pub fn row(
        kind: ChangeKind,
        id: Uuid,
        slug: &str,
        before: Option<Value>,
        after: Option<Value>,
    ) -> ChangeItem {
        ChangeItem {
            kind,
            item_id: Some(id),
            name: slug.to_string(),
            before,
            after,
            before_bytes: None,
            after_bytes: None,
            revision: None,
        }
    }

    /// A project folder file's change.
    pub fn file(path: &str, before: Option<Vec<u8>>, after: Option<Vec<u8>>) -> ChangeItem {
        ChangeItem {
            kind: ChangeKind::File,
            item_id: None,
            name: path.to_string(),
            before: None,
            after: None,
            before_bytes: before,
            after_bytes: after,
            revision: None,
        }
    }

    /// A note `name` (a plain file name) on a change, e.g. what else the change undid.
    pub fn note(name: &str, text: &str) -> ChangeItem {
        ChangeItem::file(&format!("{NOTES}{name}"), None, Some(text.into()))
    }

    /// The same, with the revision the change left its row at.
    pub fn at_revision(self, revision: i64) -> ChangeItem {
        ChangeItem {
            revision: Some(revision),
            ..self
        }
    }

    /// Whether it is a note rather than something the change did.
    pub fn is_note(&self) -> bool {
        self.kind == ChangeKind::File && self.name.starts_with(NOTES)
    }

    /// The file it is, or the file the row would be in a project folder.
    pub fn file_name(&self) -> String {
        match (self.kind, self.kind.item_kind(), Id::parse(&self.name)) {
            (ChangeKind::Project, ..) => PROJECT_FILE.to_string(),
            (_, Some(kind), Ok(id)) => item_file(kind, &id),
            _ => self.name.clone(),
        }
    }
}

/// One change as the history keeps it.
#[derive(Debug, Clone, Copy)]
pub struct Recorded<'a> {
    pub id: Uuid,
    pub label: &'a str,
    pub items: &'a [ChangeItem],
}

impl ChangeSet {
    /// What the history keeps of this change once applied to project `project`: the settings
    /// (left at `revision`), each row at the id and revision the apply left it (`rows`, in the
    /// order of `items`), then each file.
    pub fn records(&self, project: Uuid, revision: i64, rows: &[Base]) -> Vec<ChangeItem> {
        let json = |s: &ProjectFile| serde_json::to_value(s).ok();
        let settings = self.settings.iter().map(|s| {
            let (before, after) = (json(&s.before), json(&s.after));
            ChangeItem::row(ChangeKind::Project, project, PROJECT_FILE, before, after)
                .at_revision(revision)
        });
        let items = self.items.iter().zip(rows).map(|(c, row)| {
            let kind = ChangeKind::of(c.kind);
            let item = ChangeItem::row(
                kind,
                row.id,
                c.slug.as_str(),
                c.before.clone(),
                c.after.clone(),
            );
            item.at_revision(row.revision)
        });
        let files = self
            .files
            .iter()
            .map(|f| ChangeItem::file(&f.path, f.before.clone(), f.after.clone()));
        settings.chain(items).chain(files).collect()
    }
}

impl Project {
    /// Undoes `change` as a new change: every item it touched goes back to what it was before.
    /// Refused while an item was edited since (its row moved past the revision the change left
    /// it at, its file or the settings no longer hold what the change wrote), or a deleted item's
    /// slug or file is taken again. Answers the files put back (a row as its file's name).
    pub fn undo(&mut self, change: Recorded<'_>) -> Result<Vec<String>, ProjectError> {
        let items: Vec<&ChangeItem> = change.items.iter().filter(|i| !i.is_note()).collect();
        self.check_undo(&items)?;
        let label = format!("undo {}", change.label);
        for item in items.iter().rev() {
            self.put_back(&label, item)?;
        }
        if let Some(set) = self.pending(&label) {
            set.undoes = Some(change.id);
        }
        self.reload()?;
        Ok(items.iter().map(|i| i.file_name()).collect())
    }

    /// Refuses an undo that would overwrite an edit made since, or an item made since.
    fn check_undo(&self, items: &[&ChangeItem]) -> Result<(), ProjectError> {
        let mut files = Vec::new();
        for item in items {
            match self.since(item) {
                Since::Same => {}
                Since::Edited => files.push(item.file_name()),
                Since::Taken => {
                    return Err(ProjectError::Occupied {
                        file: item.file_name(),
                    })
                }
            }
        }
        match files.is_empty() {
            true => Ok(()),
            false => Err(ProjectError::EditedSince { files }),
        }
    }

    /// How `item` stands now against what its change left.
    fn since(&self, item: &ChangeItem) -> Since {
        let gone = item.after.is_none() && item.after_bytes.is_none();
        match (item.kind, item.kind.item_kind()) {
            (ChangeKind::Project, _) => match serde_json::to_value(&self.config).ok() == item.after
                && self.db.as_ref().map(|db| db.revision) == item.revision
            {
                true => Since::Same,
                false => Since::Edited,
            },
            (_, Some(kind)) if !self.tables().has(kind) => Since::Edited,
            (_, Some(kind)) if gone => match Id::parse(&item.name) {
                Ok(id) if !self.has_row(kind, &id) => Since::Same,
                _ => Since::Taken,
            },
            (_, Some(_)) => match self.row_revision(item.item_id) == item.revision {
                true => Since::Same,
                false => Since::Edited,
            },
            _ if gone => match self.store.exists(&item.name) {
                true => Since::Taken,
                false => Since::Same,
            },
            _ => match self.store.read(&item.name) {
                Ok(now) if Some(&now) != item.after_bytes.as_ref() => Since::Edited,
                Ok(_) => Since::Same,
                Err(_) => Since::Edited,
            },
        }
    }

    /// Puts `item` back as it was before its change.
    fn put_back(&mut self, label: &str, item: &ChangeItem) -> Result<(), ProjectError> {
        if item.kind == ChangeKind::Project {
            let before = item.before.clone().ok_or_else(|| unreadable(item))?;
            let config = serde_json::from_value(before).map_err(|_| unreadable(item))?;
            self.change_settings(config);
            return Ok(());
        }
        if let Some(kind) = item.kind.item_kind() {
            let id = Id::parse(&item.name)?;
            self.write_row(label, kind, &id, item.before.clone(), None)?;
            self.restore_identity(label, kind, &id, item)?;
            return Ok(());
        }
        match &item.before_bytes {
            Some(bytes) => self.write_file(label, &item.name, bytes),
            None if self.store.exists(&item.name) => self.remove_file(label, &item.name),
            None => Ok(()),
        }
    }
    fn restore_identity(
        &mut self,
        label: &str,
        kind: Kind,
        id: &Id,
        item: &ChangeItem,
    ) -> Result<(), ProjectError> {
        if item.after.is_none() {
            let base = Base {
                id: item.item_id.ok_or_else(|| unreadable(item))?,
                revision: item.revision.ok_or_else(|| unreadable(item))?,
            };
            let set = self
                .pending(label)
                .expect("undoing a row requires a table home");
            if let Some(change) = set
                .items
                .iter_mut()
                .find(|c| c.kind == kind && c.slug == *id)
            {
                change.base = Some(base);
                change.restore = true;
            }
        }
        Ok(())
    }
}

/// How an item stands against what a change left.
enum Since {
    Same,
    /// Edited since: an undo would lose the edit.
    Edited,
    /// Deleted by the change and made again since.
    Taken,
}

fn unreadable(item: &ChangeItem) -> ProjectError {
    let why = format!(
        "the history's {} {} cannot be read back",
        item.kind.as_str(),
        item.name
    );
    FormatError::new(PROJECT_FILE, "", why).into()
}
