//! A store whose writes wait: reads see them at once, the store under it only when they are
//! flushed. A home that keeps some kinds as rows changes its files through one, so a change
//! refused by the database (a row changed meanwhile) leaves the folder as it was.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use super::{check_dir, check_path, Change, Entry, Store};

/// Writes held over `base` until [`Overlay::flush`].
#[derive(Debug)]
pub struct Overlay {
    base: Arc<dyn Store>,
    /// Each path written (`Some`) or removed (`None`), by path.
    held: Mutex<BTreeMap<String, Option<Arc<[u8]>>>>,
}

impl Overlay {
    pub fn new(base: Arc<dyn Store>) -> Overlay {
        Overlay {
            base,
            held: Mutex::default(),
        }
    }

    /// The held writes, in path order (`None`: removed).
    pub fn changes(&self) -> Vec<Change> {
        self.lock().clone().into_iter().collect()
    }

    /// Writes every held change to the store under it, in path order.
    pub fn flush(&self) -> io::Result<()> {
        for (path, bytes) in self.changes() {
            match bytes {
                Some(b) => self.base.write(&path, &b)?,
                None => match self.base.remove(&path) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                },
            }
        }
        self.lock().clear();
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Option<Arc<[u8]>>>> {
        self.held.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The held paths under folder `dir` (`""`: all), with what each holds.
    fn held_under(&self, dir: &str) -> Vec<(String, Option<Arc<[u8]>>)> {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let held = self.lock();
        let range = held.range(prefix.clone()..);
        let under = range.take_while(|(p, _)| p.starts_with(&prefix));
        under
            .map(|(p, b)| (p[prefix.len()..].to_string(), b.clone()))
            .collect()
    }

    /// Every file under folder `dir`, as the store under it has them, by path.
    fn base_files(&self, dir: &str) -> Vec<String> {
        let mut out = Vec::new();
        for e in self.base.list(dir).unwrap_or_default() {
            let path = format!("{dir}/{}", e.name);
            match e.dir {
                true => out.extend(self.base_files(&path)),
                false => out.push(path),
            }
        }
        out
    }
}

fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("no {path:?}"))
}

fn file(name: &str, size: usize) -> Entry {
    Entry {
        name: name.to_string(),
        dir: false,
        link: false,
        size: size as u64,
    }
}

fn folder(name: &str) -> Entry {
    Entry {
        dir: true,
        ..file(name, 0)
    }
}

impl Store for Overlay {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        check_path(path)?;
        match self.lock().get(path) {
            Some(Some(b)) => Ok(b.to_vec()),
            Some(None) => Err(not_found(path)),
            None => self.base.read(path),
        }
    }

    fn read_head(&self, dir: &str, file: &str, max: u64) -> io::Result<(Vec<u8>, u64)> {
        if !self.lock().contains_key(&format!("{dir}/{file}")) {
            return self.base.read_head(dir, file, max);
        }
        let mut bytes = self.read(&format!("{dir}/{file}"))?;
        let size = bytes.len() as u64;
        bytes.truncate(usize::try_from(max).unwrap_or(usize::MAX));
        Ok((bytes, size))
    }

    fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        check_dir(dir)?;
        let (base, missing) = match self.base.list(dir) {
            Ok(entries) => (entries, false),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (Vec::new(), true),
            Err(e) => return Err(e),
        };
        let mut found: BTreeMap<String, Entry> =
            base.into_iter().map(|e| (e.name.clone(), e)).collect();
        for (rest, bytes) in self.held_under(dir) {
            match (rest.split_once('/'), bytes) {
                (Some((name, _)), Some(_)) => {
                    found
                        .entry(name.to_string())
                        .or_insert_with(|| folder(name));
                }
                (None, Some(b)) => {
                    found.insert(rest.clone(), file(&rest, b.len()));
                }
                (None, None) => {
                    found.remove(&rest);
                }
                (Some(_), None) => {}
            }
        }
        if missing && found.is_empty() && !dir.is_empty() {
            return Err(not_found(dir));
        }
        Ok(found.into_values().collect())
    }

    fn stat(&self, path: &str) -> Option<Entry> {
        check_path(path).ok()?;
        let name = path.rsplit('/').next().unwrap_or(path);
        match self.lock().get(path) {
            Some(Some(b)) => return Some(file(name, b.len())),
            Some(None) => return None,
            None => {}
        }
        let held_inside = self.held_under(path).iter().any(|(_, b)| b.is_some());
        match self.base.stat(path) {
            Some(e) => Some(e),
            None => held_inside.then(|| folder(name)),
        }
    }

    fn write(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        check_path(path)?;
        self.lock().insert(path.to_string(), Some(Arc::from(bytes)));
        Ok(())
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        if !self.is_file(path) {
            return Err(not_found(path));
        }
        self.lock().insert(path.to_string(), None);
        Ok(())
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let bytes = self.read(from)?;
        if from != to {
            self.write(to, &bytes)?;
            self.remove(from)?;
        }
        Ok(())
    }

    fn remove_dir(&self, dir: &str) -> io::Result<()> {
        check_path(dir)?;
        let held = self
            .held_under(dir)
            .into_iter()
            .map(|(rest, _)| format!("{dir}/{rest}"));
        let doomed: Vec<String> = self.base_files(dir).into_iter().chain(held).collect();
        if doomed.is_empty() {
            return Err(not_found(dir));
        }
        let mut held = self.lock();
        for path in doomed {
            held.insert(path, None);
        }
        Ok(())
    }

    fn make_dir(&self, dir: &str) -> io::Result<()> {
        self.base.make_dir(dir)
    }

    fn folder(&self) -> Option<&Path> {
        self.base.folder()
    }
}
