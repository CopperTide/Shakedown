//! `save.json`: which volumes a save holds and how, and the helper image its hard-linked trees
//! are read and removed with (for digests and for dropping it). A save made before strategies has none: its volumes are the
//! `.tar` archives in it.

use std::collections::BTreeMap;
use std::path::Path;

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
    /// The variables of the instance's config that steer the `docker` CLI (`PATH` and
    /// `DOCKER_*`; see `docker_env`): how to run the helper that reads or removes its trees once
    /// that instance is gone. No other variable (a compose file's secrets) is kept.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// The variables of `env` the helper's `docker` needs: `PATH` (which `docker`) and `DOCKER_*`
/// (which daemon).
pub fn docker_env(env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let keep = |k: &String| k == "PATH" || k.starts_with("DOCKER_");
    (env.iter().filter(|(k, _)| keep(k)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
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

    /// A project to run the trees' helper with (helpers only); None when the save has no trees.
    pub fn tree_helper(&self) -> Result<Option<Project>> {
        let Some(image) = self.tree_image() else {
            return Ok(None);
        };
        let project = Project::helpers_only("arena_saves", &self.env)?;
        Ok(Some(project.with_helper(image)?))
    }

    /// The image that handles its trees, when it has any.
    pub fn tree_image(&self) -> Option<&str> {
        let trees = self.volumes.iter().any(|v| v.save == SaveHow::Hardlink);
        self.helper_image.as_deref().filter(|_| trees)
    }
}
