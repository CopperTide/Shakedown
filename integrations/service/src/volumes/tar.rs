//! `tar` volumes: an archive `<volume>.tar` streamed by the tar helper, exactly as the volume is.

use std::path::{Path, PathBuf};

use anyhow::Result;
use arena_part::helpers::compose::Project;

/// Where the archive of `volume` is in the folder `dir`.
pub fn tar_path(dir: &Path, volume: &str) -> PathBuf {
    dir.join(format!("{volume}.tar"))
}

/// Writes `volume` to `dir/<volume>.tar`; the bytes written.
pub fn save(project: &Project, volume: &str, dir: &Path) -> Result<u64> {
    let tar = tar_path(dir, volume);
    project.export_volume(volume, &tar)?;
    Ok(std::fs::metadata(&tar)?.len())
}

/// Fills `volume` from `save/<volume>.tar`.
pub fn restore(project: &Project, volume: &str, save: &Path) -> Result<()> {
    project.import_volume(volume, &tar_path(save, volume))
}
