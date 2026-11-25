//! A project folder on disk. A write never leaves a file half-written: the new contents go to a
//! temporary file in the same folder, are flushed to disk, then renamed over the old file.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use super::{check_dir, check_path, Entry, Store};

/// The project in folder `root`.
#[derive(Debug, Clone)]
pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    pub fn new(root: &Path) -> FsStore {
        FsStore {
            root: root.to_path_buf(),
        }
    }

    /// `path` under the folder, once checked.
    fn at(&self, path: &str) -> io::Result<PathBuf> {
        check_path(path)?;
        Ok(self.root.join(path))
    }
}

impl Store for FsStore {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        std::fs::read(self.at(path)?)
    }

    fn read_head(&self, dir: &str, file: &str, max: u64) -> io::Result<(Vec<u8>, u64)> {
        let base = self.at(dir)?.canonicalize()?;
        let path = self.at(&format!("{dir}/{file}"))?.canonicalize()?;
        if !path.starts_with(&base) {
            let why = format!("{file:?} is outside {dir}");
            return Err(io::Error::new(io::ErrorKind::InvalidInput, why));
        }
        let opened = std::fs::File::open(&path)?;
        let size = opened.metadata()?.len();
        let mut bytes = Vec::new();
        opened.take(max).read_to_end(&mut bytes)?;
        Ok((bytes, size))
    }

    fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        check_dir(dir)?;
        let mut found = Vec::new();
        for entry in std::fs::read_dir(self.root.join(dir))? {
            let entry = entry?;
            let meta = std::fs::metadata(entry.path());
            found.push(Entry {
                name: entry.file_name().to_string_lossy().to_string(),
                dir: meta.as_ref().is_ok_and(std::fs::Metadata::is_dir),
                link: entry.file_type().is_ok_and(|t| t.is_symlink()),
                size: meta.map_or(0, |m| if m.is_dir() { 0 } else { m.len() }),
            });
        }
        Ok(found)
    }

    fn stat(&self, path: &str) -> Option<Entry> {
        let at = self.at(path).ok()?;
        let meta = std::fs::metadata(&at).ok()?;
        Some(Entry {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            dir: meta.is_dir(),
            link: std::fs::symlink_metadata(&at).is_ok_and(|m| m.is_symlink()),
            size: if meta.is_dir() { 0 } else { meta.len() },
        })
    }

    fn write(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        let path = self.at(path)?;
        let dir = path.parent().unwrap_or(&self.root);
        std::fs::create_dir_all(dir)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let temp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
        let result = write_and_sync(&temp, bytes).and_then(|()| std::fs::rename(&temp, &path));
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        std::fs::remove_file(self.at(path)?)
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let (from, to) = (self.at(from)?, self.at(to)?);
        std::fs::create_dir_all(to.parent().unwrap_or(&self.root))?;
        std::fs::rename(from, to)
    }

    fn remove_dir(&self, dir: &str) -> io::Result<()> {
        std::fs::remove_dir_all(self.at(dir)?)
    }

    fn make_dir(&self, dir: &str) -> io::Result<()> {
        std::fs::create_dir_all(self.at(dir)?)
    }

    fn folder(&self) -> Option<&Path> {
        Some(&self.root)
    }
}

fn write_and_sync(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(contents)?;
    file.sync_all()
}
