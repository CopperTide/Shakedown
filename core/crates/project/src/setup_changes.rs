//! A write to a table setup's auxiliary files also versions its owning setup.

use crate::{format::Kind, ChangeSet, Id, ItemChange, Project, ProjectError};
use std::collections::BTreeSet;

impl Project {
    /// Include the owning setup row for every changed setup file, so file-only edits participate
    /// in that setup's revision check. Homes call this after collecting direct store writes.
    pub fn track_setup_files(&self, change: &mut ChangeSet) -> Result<(), ProjectError> {
        if !self.tables().setups {
            return Ok(());
        }
        let ids: BTreeSet<Id> = change
            .files
            .iter()
            .filter_map(|f| {
                let (id, _) = f.path.strip_prefix("setups/")?.split_once('/')?;
                Id::parse(id).ok()
            })
            .collect();
        for id in ids {
            if !change
                .items
                .iter()
                .any(|i| i.kind == Kind::Setup && i.slug == id)
            {
                change.items.push(self.setup_file_owner(&id)?);
            }
        }
        Ok(())
    }

    fn setup_file_owner(&self, id: &Id) -> Result<ItemChange, ProjectError> {
        let row = self
            .db
            .as_ref()
            .and_then(|db| db.rows.get(&(Kind::Setup, id.clone())))
            .ok_or_else(|| ProjectError::not_found(Kind::Setup, id))?;
        Ok(ItemChange {
            kind: Kind::Setup,
            slug: id.clone(),
            base: row.base,
            restore: false,
            before: Some(row.body.clone()),
            after: Some(row.body.clone()),
        })
    }
}
