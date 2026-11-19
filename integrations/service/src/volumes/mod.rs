//! Moving an instance's volumes into saves and back, each its own way (see `SaveHow`): `tar`
//! archives, `postgres` dumps, `hardlink` trees, or not at all (`skip`).
mod hardlink;
mod manifest;
mod postgres;
mod tar;

use std::path::Path;

use anyhow::{bail, Result};
use arena_part::helpers::compose::Project;
use arena_part::helpers::folder;

use crate::config::{SaveHow, VolumeSpec};
pub use manifest::{Manifest, SavedVolume};
pub use postgres::{digest as dump_digest, sql_path};
pub use tar::tar_path;

/// A running instance: its project, the volumes a save holds and the helper image that copies
/// them.
#[derive(Debug, Clone)]
pub struct Instance {
    pub project: Project,
    pub volumes: Vec<VolumeSpec>,
    pub helper_image: String,
    /// The manifest fields that let a helper reach the save's trees later.
    pub compose_file: std::path::PathBuf,
    pub env: std::collections::BTreeMap<String, String>,
}

impl Instance {
    fn of(&self, how: SaveHow) -> impl Iterator<Item = &VolumeSpec> {
        self.volumes.iter().filter(move |v| v.save == how)
    }

    /// The compose services to stop while files are copied: every file volume's `services`
    /// when each names some, else None (the whole project).
    fn copy_services(&self) -> Option<Vec<&str>> {
        let copies: Vec<&VolumeSpec> = self.volumes.iter().filter(|v| v.is_file_copy()).collect();
        if copies.iter().any(|v| v.services.is_empty()) {
            return None;
        }
        let mut s: Vec<&str> = vec![];
        for name in copies.iter().flat_map(|v| &v.services) {
            if !s.contains(&name.as_str()) {
                s.push(name);
            }
        }
        Some(s)
    }

    fn manifest(&self) -> Manifest {
        let volumes = (self.volumes.iter().filter(|v| v.save != SaveHow::Skip))
            .map(|v| SavedVolume {
                name: v.name.clone(),
                save: v.save,
            })
            .collect();
        Manifest {
            volumes,
            helper_image: Some(self.helper_image.clone()),
            compose_file: Some(self.compose_file.clone()),
            env: self.env.clone(),
        }
    }
}

/// Writes every volume of a live instance to `into` for its digests (`into` must exist): file
/// volumes as `<volume>.tar`, databases as `<volume>.sql`; the bytes written.
pub fn export_all(inst: &Instance, into: &Path) -> Result<u64> {
    let mut bytes = 0;
    for v in &inst.volumes {
        bytes += match v.save {
            SaveHow::Tar | SaveHow::Hardlink => tar::save(&inst.project, &v.name, into)?,
            SaveHow::Postgres => postgres::save(&inst.project, v, into)?,
            SaveHow::Skip => 0,
        };
    }
    Ok(bytes)
}

/// Writes every volume to `dir` (databases dumped while they run, then files copied with the
/// services that use them stopped), and the manifest; the bytes written. `prev` is the save the
/// instance's hard-linked trees link to. A start is always attempted after a stop, and the first
/// error is returned; a failed save leaves no folder behind.
pub fn save_all(inst: &Instance, dir: &Path, prev: Option<&str>) -> Result<u64> {
    let saved = std::fs::create_dir_all(dir)
        .map_err(anyhow::Error::from)
        .and_then(|()| save_into(inst, dir, prev));
    if saved.is_err() {
        discard(inst, dir);
    }
    saved
}

fn save_into(inst: &Instance, dir: &Path, prev: Option<&str>) -> Result<u64> {
    let mut bytes = 0;
    for v in inst.of(SaveHow::Postgres) {
        bytes += postgres::save(&inst.project, v, dir)?;
    }
    if inst.volumes.iter().any(VolumeSpec::is_file_copy) {
        bytes += copy_files(inst, dir, prev)?;
    }
    inst.manifest().write(dir)?;
    Ok(bytes)
}

/// Copies the file volumes with what they need stopped, then starts it again.
fn copy_files(inst: &Instance, dir: &Path, prev: Option<&str>) -> Result<u64> {
    let services = inst.copy_services();
    let stopped = match &services {
        Some(s) => inst.project.stop_services(s),
        None => inst.project.stop(),
    };
    let copied = stopped.and_then(|()| copy_each(inst, dir, prev));
    let started = match &services {
        Some(s) => inst.project.start_services(s),
        None => inst.project.start(),
    };
    copied.and_then(|bytes| started.map(|()| bytes))
}

fn copy_each(inst: &Instance, dir: &Path, prev: Option<&str>) -> Result<u64> {
    let mut bytes = 0;
    for v in inst.volumes.iter().filter(|v| v.is_file_copy()) {
        match v.save {
            SaveHow::Hardlink => {
                let prev = prev.filter(|p| has_tree(dir, p, &v.name));
                hardlink::save(&inst.project, &v.name, dir, prev)?
            }
            _ => bytes += tar::save(&inst.project, &v.name, dir)?,
        }
    }
    Ok(bytes)
}

/// True when the save `prev` next to `dir` holds `volume` as a tree.
fn has_tree(dir: &Path, prev: &str, volume: &str) -> bool {
    let prev = dir.with_file_name(prev);
    Manifest::read(&prev).is_some_and(|m| m.has_tree(volume))
}

/// Removes a failed save's folder (its trees through the helper).
fn discard(inst: &Instance, dir: &Path) {
    if inst.of(SaveHow::Hardlink).next().is_some() {
        let _ = hardlink::remove(&inst.project, dir);
    }
    folder::discard(dir);
}

/// Creates the project's volumes, fills them from `save`, and starts the project: file volumes
/// first; databases are loaded once their services run, before the rest starts.
pub fn restore_all(inst: &Instance, save: &Path) -> Result<()> {
    check_save(inst, save)?;
    inst.project.create()?;
    for v in &inst.volumes {
        match v.save {
            SaveHow::Tar => tar::restore(&inst.project, &v.name, save)?,
            SaveHow::Hardlink => hardlink::restore(&inst.project, &v.name, save)?,
            SaveHow::Postgres | SaveHow::Skip => {}
        }
    }
    let databases: Vec<&VolumeSpec> = inst.of(SaveHow::Postgres).collect();
    if databases.is_empty() {
        return inst.project.start();
    }
    let services: Vec<&str> = databases
        .iter()
        .filter_map(|v| v.service.as_deref())
        .collect();
    inst.project.up_services(&services)?;
    for v in databases {
        postgres::restore(&inst.project, v, save)?;
    }
    inst.project.up_services(&[])
}

/// Every volume the save must hold is there.
fn check_save(inst: &Instance, save: &Path) -> Result<()> {
    let missing = inst.volumes.iter().find(|v| match v.save {
        SaveHow::Tar => !tar_path(save, &v.name).is_file(),
        SaveHow::Postgres => !sql_path(save, &v.name).is_file(),
        SaveHow::Hardlink => !save.join(&v.name).is_dir(),
        SaveHow::Skip => false,
    });
    match missing {
        Some(v) => bail!("the save has no volume {}", v.name),
        None => Ok(()),
    }
}

/// Removes the trees of the save in `dir` through their helper, when it has any.
pub fn remove_trees(dir: &Path) -> Result<()> {
    let Some(m) = Manifest::read(dir) else {
        return Ok(());
    };
    match m.tree_helper()? {
        Some(project) => hardlink::remove(&project, dir),
        None => Ok(()),
    }
}

/// Writes the tree of `volume` in the save `dir` as an archive at `dest`.
pub fn archive_tree(dir: &Path, volume: &str, dest: &Path) -> Result<()> {
    let Some(m) = Manifest::read(dir) else {
        bail!("the save has no manifest for the tree {volume}");
    };
    match m.tree_helper()? {
        Some(project) => project.export_dir(&dir.join(volume), dest),
        None => bail!("the save has no helper for the tree {volume}"),
    }
}
