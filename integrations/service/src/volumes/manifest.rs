//! `save.json`: which volumes a save holds and how, and the helper image its hard-linked trees
//! are read and removed with (for digests and for dropping it). A save made before strategies has none: its volumes are the
//! `.tar` archives in it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use arena_part::helpers::compose::Project;
use serde::{Deserialize, Serialize};

use crate::config::SaveHow;

const FILE: &str = "save.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedVolume {
    pub name: String,
    pub save: SaveHow,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub volumes: Vec<SavedVolume>,
    /// The helper image (saves made before it was the helper name their rsync image).
    #[serde(
        default,
        alias = "rsync_image",
        skip_serializing_if = "Option::is_none"
    )]
    pub helper_image: Option<String>,
    /// The compose file and environment of the instance that made the save: how to run the
    /// helper that reads or removes its trees once that instance is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

impl Manifest {
    pub fn write(&self, dir: &Path) -> Result<()> {
        let text = serde_json::to_string(self)?;
        std::fs::write(dir.join(FILE), text).with_context(|| format!("writing {FILE}"))
    }

    /// The manifest of the save in `dir`; None for a save made before strategies.
    pub fn read(dir: &Path) -> Option<Manifest> {
        let text = std::fs::read_to_string(dir.join(FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// True when the save holds `volume` as a hard-linked tree.
    pub fn has_tree(&self, volume: &str) -> bool {
        (self.volumes.iter()).any(|v| v.name == volume && v.save == SaveHow::Hardlink)
    }

    /// A project to run the trees' helper with (never brought up); None when the save has no
    /// trees.
    pub fn tree_helper(&self) -> Result<Option<Project>> {
        let (Some(image), Some(file)) = (self.tree_image(), &self.compose_file) else {
            return Ok(None);
        };
        Ok(Some(
            Project::new("arena_saves", file, &self.env)?.with_helper(image)?,
        ))
    }

    /// The image that handles its trees, when it has any.
    pub fn tree_image(&self) -> Option<&str> {
        let trees = self.volumes.iter().any(|v| v.save == SaveHow::Hardlink);
        self.helper_image.as_deref().filter(|_| trees)
    }
}
