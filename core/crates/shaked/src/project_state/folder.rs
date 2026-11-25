//! What changes whenever a file of a project folder is added, removed or edited.

use std::path::Path;
use std::time::SystemTime;

use shakedown_project::format::Kind;

/// The newest modification time and the number of entries under the folders a project reads.
pub(super) fn stamp(root: &Path) -> (Option<SystemTime>, usize) {
    let mut dirs = vec![root.to_path_buf()];
    dirs.extend(Kind::ALL.iter().map(|k| root.join(k.dir())));
    let worlds = std::fs::read_dir(root.join(Kind::World.dir()))
        .into_iter()
        .flatten()
        .flatten();
    dirs.extend(worlds.map(|e| e.path()).filter(|p| p.is_dir()));
    let mut newest = None;
    let mut count = 0;
    for entry in dirs
        .iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
    {
        count += 1;
        let modified = entry.metadata().and_then(|m| m.modified()).ok();
        newest = newest.max(modified);
    }
    (newest, count)
}
