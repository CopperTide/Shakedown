//! `hardlink` volumes: the volume's files copied into the save as a tree by `rsync` in a helper
//! container, hard-linked to the instance's previous save where a file is unchanged, so a save
//! costs only what changed. The trees belong to the container's users: they are read, restored
//! and removed through the helper too.

use std::path::Path;

use anyhow::{Context, Result};
use arena_part::helpers::compose::Project;

/// Where a helper sees the package's save folders.
const SAVES: &str = "/saves";

/// A save folder's root (the package folder) and its name.
fn split(dir: &Path) -> Result<(String, String)> {
    let root = dir.parent().context("a save folder has a parent")?;
    let name = dir.file_name().context("a save folder has a name")?;
    let root = root.to_string_lossy().into_owned();
    Ok((root, name.to_string_lossy().into_owned()))
}

/// Copies `volume` into `dir/<volume>/`, linking to the save `prev` (in the same package folder)
/// where files are unchanged.
pub fn save(
    project: &Project,
    image: &str,
    volume: &str,
    dir: &Path,
    prev: Option<&str>,
) -> Result<()> {
    let (root, save) = split(dir)?;
    let mounts = [
        (project.volume(volume)?, "/v".to_string(), true),
        (root, SAVES.to_string(), false),
    ];
    let link = prev.map(|p| format!("--link-dest={SAVES}/{p}/{volume}/"));
    let to = format!("{SAVES}/{save}/{volume}/");
    let mut args = vec!["rsync", "-a", "--delete", "--numeric-ids"];
    args.extend(link.as_deref());
    args.extend(["/v/", to.as_str()]);
    project.run_helper(image, &mounts, &args).map(drop)
}

/// Fills `volume` from the tree `save/<volume>/` (mounted read-only).
pub fn restore(project: &Project, image: &str, volume: &str, save: &Path) -> Result<()> {
    let tree = save.join(volume).to_string_lossy().into_owned();
    let mounts = [
        (tree, "/from".to_string(), true),
        (project.volume(volume)?, "/v".to_string(), false),
    ];
    project
        .run_helper(image, &mounts, &["cp", "-a", "/from/.", "/v/"])
        .map(drop)
}

/// Writes the tree `save/<volume>/` as a tar archive to `dest` (for its digest).
pub fn archive(
    project: &Project,
    image: &str,
    volume: &str,
    save: &Path,
    dest: &Path,
) -> Result<()> {
    let tree = save.join(volume).to_string_lossy().into_owned();
    let mounts = [(tree, "/t".to_string(), true)];
    let args = ["tar", "-C", "/t", "-cf", "-", "--numeric-owner", "."];
    project.run_helper_into(image, &mounts, &args, dest)
}

/// Removes the save folder `dir` (its trees too) through the helper.
pub fn remove(project: &Project, image: &str, dir: &Path) -> Result<()> {
    let (root, save) = split(dir)?;
    let target = format!("{SAVES}/{save}");
    let mounts = [(root, SAVES.to_string(), false)];
    project
        .run_helper(image, &mounts, &["rm", "-rf", &target])
        .map(drop)
}
