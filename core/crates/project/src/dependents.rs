//! What uses an item, and deleting an item together with what uses it. The whole delete is planned
//! (and every rewritten file checked) before anything changes, then carried out from the leaves
//! in: rewritten experiments, deleted experiments, deleted scenarios, the item. A failure part way
//! leaves no reference to a missing item. Deleted files go to the trash, so a delete can be undone.

use std::collections::BTreeSet;

use crate::error::ProjectError;
use crate::format::{ExperimentFile, Kind};
use crate::item::{kind_name, Item, ItemBody};
use crate::load::item_file;
use crate::project::checked_yaml;
use crate::trash::{Bin, Manifest};
use crate::validate::references;
use crate::write::write_atomic;
use crate::{Id, Project};

/// What deleting an item with cascade does to one item that uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fate {
    /// It is deleted too.
    Deleted,
    /// An experiment that compares other setups too: the deleted setup leaves its `setups`.
    SetupDropped,
    /// A sub-experiment of a deleted experiment: it loses its `parent` and stands on its own.
    ParentCleared,
}

/// One item that uses another, and what a cascading delete of the other does to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependent {
    pub kind: Kind,
    pub id: Id,
    pub label: String,
    /// `[Deleted]`, or every change to an item that stays (it can lose a setup and its parent).
    pub fates: Vec<Fate>,
}

impl Dependent {
    /// Whether the delete deletes it too.
    pub fn deleted(&self) -> bool {
        self.fates.contains(&Fate::Deleted)
    }
}

/// A dependent as a preview showed it: its kind, id and fates.
pub type Expected = (Kind, Id, Vec<Fate>);

/// What a delete did: what happened to each item that used the item, and the trash folder the
/// deleted files went to (see [`Project::restore`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub fallout: Vec<Dependent>,
    pub trash: String,
}

/// Items by kind and id.
type Keys = BTreeSet<(Kind, Id)>;

impl Project {
    /// The items that name item `id` of `kind`, directly or through an item deleted with it,
    /// scenarios before experiments, each in id order: a world's scenarios and their
    /// experiments; a scenario's experiments; the experiments comparing a setup; an experiment's
    /// sub-experiments.
    pub fn dependents(&self, kind: Kind, id: &Id) -> Vec<(Kind, Id)> {
        let fallout = self.fallout(kind, id);
        fallout.into_iter().map(|d| (d.kind, d.id)).collect()
    }

    /// What deleting item `id` of `kind` with cascade would do to each item that uses it.
    pub fn fallout(&self, kind: Kind, id: &Id) -> Vec<Dependent> {
        self.assess(kind, id).1
    }

    /// Deletes item `id` of `kind`. While anything uses it, refuses unless `cascade`; with
    /// `expect` (what a preview showed), also refuses when what uses it has changed since. A
    /// world's data folder and a setup's folder stay; only the item's YAML file goes.
    pub fn delete(
        &mut self,
        kind: Kind,
        id: &Id,
        cascade: bool,
        expect: Option<&[Expected]>,
    ) -> Result<Removal, ProjectError> {
        let (doomed, fallout) = self.allowed(kind, id, cascade, expect)?;
        let edits = self.edits(&fallout, &doomed)?;
        let deletes = deletions(kind, id, &fallout);
        let manifest = Manifest::new(format!("{} {id}", kind_name(kind)), &deletes, &edits);
        let bin = Bin::open(&self.root, &manifest)?;
        for (file, yaml) in &edits {
            bin.keep_copy(file)?;
            write_atomic(&self.root, file, yaml)?;
        }
        for file in &deletes {
            bin.take(file)?;
        }
        self.reload()?;
        Ok(Removal {
            fallout,
            trash: bin.name,
        })
    }

    /// What a delete of item `id` of `kind` takes and does, unless the delete is refused: the item
    /// is missing, something uses it and `cascade` is off, or what uses it is no longer `expect`.
    fn allowed(
        &self,
        kind: Kind,
        id: &Id,
        cascade: bool,
        expect: Option<&[Expected]>,
    ) -> Result<(Keys, Vec<Dependent>), ProjectError> {
        if self.get(kind, id).is_none() {
            return Err(ProjectError::not_found(kind, id));
        }
        let (doomed, by) = self.assess(kind, id);
        let (kind, id) = (kind_name(kind), id.to_string());
        if !by.is_empty() && !cascade {
            return Err(ProjectError::InUse { kind, id, by });
        }
        if expect.is_some_and(|e| !same(e, &by)) {
            return Err(ProjectError::Stale { kind, id, by });
        }
        Ok((doomed, by))
    }

    /// The items a delete of `id` takes with it (the item included), and what happens to every
    /// item that uses one of them.
    fn assess(&self, kind: Kind, id: &Id) -> (Keys, Vec<Dependent>) {
        let doomed = self.doomed(kind, id);
        let fallout = self.users().filter_map(|item| {
            let key = (item.kind, item.id.clone());
            let fates = match doomed.contains(&key) {
                true if key != (kind, id.clone()) => vec![Fate::Deleted],
                true => Vec::new(),
                false => changed(&item.body, &doomed),
            };
            (!fates.is_empty()).then(|| Dependent {
                kind: item.kind,
                id: item.id.clone(),
                label: item.label.clone(),
                fates,
            })
        });
        let fallout = fallout.collect();
        (doomed, fallout)
    }

    /// The item and everything deleted with it, found until nothing more has to go.
    fn doomed(&self, kind: Kind, id: &Id) -> Keys {
        let mut doomed = Keys::from([(kind, id.clone())]);
        loop {
            let more: Vec<(Kind, Id)> = self
                .users()
                .map(|i| (i, (i.kind, i.id.clone())))
                .filter(|(i, key)| !doomed.contains(key) && must_go(&i.body, &doomed))
                .map(|(_, key)| key)
                .collect();
            if more.is_empty() {
                return doomed;
            }
            doomed.extend(more);
        }
    }

    /// The items that can use another: scenarios, then experiments.
    fn users(&self) -> impl Iterator<Item = &Item> {
        let scenarios = self.list(Kind::Scenario).into_iter();
        scenarios.chain(self.list(Kind::Experiment))
    }

    /// The experiments a delete rewrites, as their files and checked new YAML.
    fn edits(
        &self,
        fallout: &[Dependent],
        doomed: &Keys,
    ) -> Result<Vec<(String, String)>, ProjectError> {
        let changed = fallout.iter().filter(|d| !d.deleted());
        changed
            .filter_map(|d| Some((&d.id, self.body::<ExperimentFile>(&d.id)?)))
            .map(|(id, e)| {
                let body = ItemBody::Experiment(without(e, doomed));
                let yaml = checked_yaml(Kind::Experiment, id, &body)?;
                Ok((item_file(Kind::Experiment, id), yaml))
            })
            .collect()
    }
}

/// The files a delete removes, leaves first: experiments, scenarios, then the item.
fn deletions(kind: Kind, id: &Id, fallout: &[Dependent]) -> Vec<String> {
    let gone = |k: Kind| {
        let of_kind = fallout.iter().filter(move |d| d.kind == k && d.deleted());
        of_kind.map(|d| item_file(d.kind, &d.id))
    };
    let mut files: Vec<String> = gone(Kind::Experiment).chain(gone(Kind::Scenario)).collect();
    files.push(item_file(kind, id));
    files
}

/// Whether `expect` names the same items with the same fates as `fallout`, in any order.
fn same(expect: &[Expected], fallout: &[Dependent]) -> bool {
    let want: BTreeSet<_> = expect.iter().cloned().collect();
    let have: BTreeSet<_> = fallout
        .iter()
        .map(|d| (d.kind, d.id.clone(), d.fates.clone()))
        .collect();
    want == have
}

/// Whether item `kind`/`name` is among `doomed`.
fn hit(doomed: &Keys, kind: Kind, name: &str) -> bool {
    Id::parse(name).is_ok_and(|n| doomed.contains(&(kind, n)))
}

/// Whether `body` goes with `doomed`: it names a doomed world or scenario, or every setup it
/// compares is doomed.
fn must_go(body: &ItemBody, doomed: &Keys) -> bool {
    let refs: Vec<(Kind, &str, String)> = references(body).collect();
    let hard = |k: Kind| matches!(k, Kind::World | Kind::Scenario);
    let gone_hard = refs.iter().any(|(k, n, _)| hard(*k) && hit(doomed, *k, n));
    let mut setups = refs.iter().filter(|(k, ..)| *k == Kind::Setup).peekable();
    let has_setups = setups.peek().is_some();
    gone_hard || (has_setups && setups.all(|(k, n, _)| hit(doomed, *k, n)))
}

/// What a delete of `doomed` does to `body` when `body` stays: it loses a setup, its parent,
/// or both; nothing when it uses none of them.
fn changed(body: &ItemBody, doomed: &Keys) -> Vec<Fate> {
    let ItemBody::Experiment(e) = body else {
        return Vec::new();
    };
    let drops_setup = e.setups.iter().any(|s| hit(doomed, Kind::Setup, s));
    let parent = e.parent.as_deref();
    let clears_parent = parent.is_some_and(|p| hit(doomed, Kind::Experiment, p));
    let fates = [
        (drops_setup, Fate::SetupDropped),
        (clears_parent, Fate::ParentCleared),
    ];
    fates
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, f)| f)
        .collect()
}

/// Experiment `e` without its references to `doomed` setups and parent.
fn without(e: &ExperimentFile, doomed: &Keys) -> ExperimentFile {
    let mut e = e.clone();
    e.setups.retain(|s| !hit(doomed, Kind::Setup, s));
    if e.parent
        .as_deref()
        .is_some_and(|p| hit(doomed, Kind::Experiment, p))
    {
        e.parent = None;
    }
    e
}
