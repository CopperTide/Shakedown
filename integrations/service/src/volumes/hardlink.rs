//! `hardlink` volumes: the volume's files copied into the save as a tree by the helper image
//! (the one that archives every volume), hard-linked to the instance's previous save where a file
//! is unchanged, so a save costs only what changed. The trees belong to the container's users:
//! they are read, restored and removed through the helper too.

use std::path::Path;

use anyhow::{Context, Result};
use arena_part::helpers::compose::Project;

/// Where a helper sees the package's save folders.
const SAVES: &str = "/saves";

/// The copy, run by the helper's `sh` as `sh -c SAVE sh <save> <prev or ""> <volume>` with the
/// volume at `/v` (read-only) and the save folders at `/saves`. Without a previous save the volume
/// is copied whole (`cp -a`). With one, the previous tree is hard-linked whole (`cp -al`), then
/// every entry that is not the same in the volume is unlinked and copied afresh. "The same" is
/// what a tar archive tells: type, size, mtime (seconds), mode and owner, and a file modified at
/// or after the second the previous save was written (`save.json`) is never the same, so a quick
/// second write is not missed. Folders get the volume's owners and modes last.
const SAVE: &str = r#"set -eu
new="/saves/$1/$3"
mkdir "$new"
sig() { find . -mindepth 1 ! -type d -exec stat -c '%Y %F %s %a %u %g|%n' {} + | sort; }
names() { sed 's/^[^|]*|//' | tr '\n' '\0'; }
folders() { find . -mindepth 1 -type d | sort; }
if [ -z "$2" ]; then
  cp -a /v/. "$new/"
else
  prev="/saves/$2/$3"
  cut=$(stat -c %Y "/saves/$2/save.json")
  (cd /v && sig) | awk -v c="$cut" '$1 >= c { print "new " $0; next } { print }' | sort > /tmp/live
  (cd "$prev" && sig) > /tmp/prev
  (cd /v && folders) > /tmp/live.d
  (cd "$prev" && folders) > /tmp/prev.d
  cp -al "$prev/." "$new/"
  cd "$new"
  comm -13 /tmp/live /tmp/prev | names | xargs -0 -r rm -f
  comm -13 /tmp/live.d /tmp/prev.d | tr '\n' '\0' | xargs -0 -r rm -rf
  tr '\n' '\0' < /tmp/live.d | xargs -0 -r mkdir -p
  comm -23 /tmp/live /tmp/prev | names | xargs -0 -r -I{} cp -a "/v/{}" "$new/{}"
fi
cd /v
find . -type d | while IFS= read -r d; do
  chown "$(stat -c %u:%g "$d")" "$new/$d"
  chmod "$(stat -c %a "$d")" "$new/$d"
done
"#;

/// A save folder's root (the package folder) and its name.
fn split(dir: &Path) -> Result<(String, String)> {
    let root = dir.parent().context("a save folder has a parent")?;
    let name = dir.file_name().context("a save folder has a name")?;
    let root = root.to_string_lossy().into_owned();
    Ok((root, name.to_string_lossy().into_owned()))
}

/// Copies `volume` into `dir/<volume>/`, linking to the save `prev` (in the same package folder)
/// where files are unchanged.
pub fn save(project: &Project, volume: &str, dir: &Path, prev: Option<&str>) -> Result<()> {
    let (root, save) = split(dir)?;
    let mounts = [
        (project.volume(volume)?, "/v".to_string(), true),
        (root, SAVES.to_string(), false),
    ];
    let args = ["sh", "-c", SAVE, "sh", &save, prev.unwrap_or(""), volume];
    project.run_helper(&mounts, &args).map(drop)
}

/// Fills `volume` from the tree `save/<volume>/` (mounted read-only).
pub fn restore(project: &Project, volume: &str, save: &Path) -> Result<()> {
    let tree = save.join(volume).to_string_lossy().into_owned();
    let mounts = [
        (tree, "/from".to_string(), true),
        (project.volume(volume)?, "/v".to_string(), false),
    ];
    project
        .run_helper(&mounts, &["cp", "-a", "/from/.", "/v/"])
        .map(drop)
}

/// Removes the save folder `dir` (its trees too) through the helper.
pub fn remove(project: &Project, dir: &Path) -> Result<()> {
    let (root, save) = split(dir)?;
    let target = format!("{SAVES}/{save}");
    let mounts = [(root, SAVES.to_string(), false)];
    project
        .run_helper(&mounts, &["rm", "-rf", &target])
        .map(drop)
}
