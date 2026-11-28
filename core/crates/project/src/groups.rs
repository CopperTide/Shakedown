//! World groups: the optional `group` a world is listed under, e.g. the customer it is for.

use crate::error::ProjectError;
use crate::format::{Kind, WorldFile};
use crate::item::{ItemBody, KindFile};
use crate::load::item_file;
use crate::project::checked_yaml;
use crate::{Id, Project};

impl Project {
    /// The worlds in group `from` as they would be in group `to` (`None`: no group), in id order.
    pub fn regrouped(&self, from: &str, to: Option<&str>) -> Vec<(Id, WorldFile)> {
        let in_group = |w: &&WorldFile| w.group.as_deref() == Some(from);
        let worlds = self.list(Kind::World).into_iter().filter_map(|item| {
            let world = WorldFile::of(&item.body).filter(in_group)?;
            let mut world = world.clone();
            world.group = to.map(str::to_string);
            Some((item.id.clone(), world))
        });
        worlds.collect()
    }

    /// Moves every world in group `from` to `to` (`None`: no group): every file is checked first,
    /// then all are written, with one re-read at the end. Answers the ids of the worlds moved;
    /// a group no world is in is not found.
    pub fn rename_group(&mut self, from: &str, to: Option<&str>) -> Result<Vec<Id>, ProjectError> {
        let moved = self.regrouped(from, to);
        if moved.is_empty() {
            return Err(ProjectError::no_group(from));
        }
        let mut files = Vec::new();
        for (id, world) in &moved {
            let body = ItemBody::World(world.clone());
            files.push((
                item_file(Kind::World, id),
                checked_yaml(Kind::World, id, &body)?,
            ));
        }
        let label = format!("rename group {from}");
        for (file, yaml) in &files {
            self.write_file(&label, file, yaml.as_bytes())?;
        }
        self.reload()?;
        Ok(moved.into_iter().map(|(id, _)| id).collect())
    }
}
