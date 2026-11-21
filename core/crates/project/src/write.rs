//! Writes a file so that a crash or failure never leaves it half-written: the new contents go to a
//! temporary file in the same folder, are flushed to disk, then renamed over the old file.

use std::io::Write;
use std::path::Path;

use crate::error::ProjectError;
use crate::load::io;

/// Replaces `root/file` with `contents` atomically, creating its folder if needed.
pub(crate) fn write_atomic(root: &Path, file: &str, contents: &str) -> Result<(), ProjectError> {
    let path = root.join(file);
    let dir = path.parent().unwrap_or(root);
    std::fs::create_dir_all(dir).map_err(|e| io("creating the folder for", file, e))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let temp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    let result = write_and_sync(&temp, contents).and_then(|()| std::fs::rename(&temp, &path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|e| io("writing", file, e))
}

fn write_and_sync(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()
}
