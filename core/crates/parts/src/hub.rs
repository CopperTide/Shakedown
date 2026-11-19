//! Lazily started part programs, one per package, shared by every branch.
use crate::client::Client;
use crate::manifest::Registry;
use crate::request;
use anyhow::{anyhow, Result};
use arena_part::helpers::remove_tree_or_log;
use arena_part::proto::{valid_id, BenchDescribe, Describe};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

struct Slot {
    client: Arc<Client>,
    describe: Option<Describe>,
    bench: Option<BenchDescribe>,
}

pub struct Hub {
    registry: Registry,
    bin_dir: PathBuf,
    saves_root: PathBuf,
    /// Extra environment for command-mode programs (e.g. `ARENA_PG_URL`).
    env: Vec<(String, String)>,
    slots: Mutex<HashMap<String, Slot>>,
}

impl Hub {
    pub fn new(registry: Registry, bin_dir: PathBuf, saves_root: PathBuf) -> Hub {
        Hub {
            registry,
            bin_dir,
            saves_root,
            env: vec![],
            slots: Mutex::default(),
        }
    }

    /// Sets environment variables every command-mode program gets, over the passed-through ones.
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Hub {
        self.env = env;
        self
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// The program of a package: started on first use, started again when it has stopped.
    pub async fn client(&self, name: &str) -> Result<Arc<Client>> {
        let mut slots = self.slots.lock().await;
        if let Some(s) = slots.get(name) {
            if !s.client.is_stopped() {
                return Ok(s.client.clone());
            }
        }
        let m = self
            .registry
            .get(name)
            .ok_or_else(|| anyhow!("no package named {name}"))?;
        let client =
            Arc::new(Client::spawn_with_env(m, &self.bin_dir, &self.saves_root, &self.env).await?);
        slots.insert(
            name.to_string(),
            Slot {
                client: client.clone(),
                describe: None,
                bench: None,
            },
        );
        Ok(client)
    }

    /// `describe` of an integration, cached until the program is respawned.
    pub async fn describe(&self, name: &str) -> Result<Describe> {
        let client = self.client(name).await?;
        if let Some(d) = self.cached(name, &client, |s| s.describe.clone()).await {
            return Ok(d);
        }
        let d: Describe = client.request(&request::Describe {}).await?;
        self.store(name, &client, |s| s.describe = Some(d.clone()))
            .await;
        Ok(d)
    }

    /// `describe` of a benchmark, cached until the program is respawned.
    pub async fn bench_describe(&self, name: &str) -> Result<BenchDescribe> {
        let client = self.client(name).await?;
        if let Some(d) = self.cached(name, &client, |s| s.bench.clone()).await {
            return Ok(d);
        }
        let d: BenchDescribe = client.request(&request::Describe {}).await?;
        self.store(name, &client, |s| s.bench = Some(d.clone()))
            .await;
        Ok(d)
    }

    async fn cached<T>(
        &self,
        name: &str,
        c: &Arc<Client>,
        f: impl Fn(&Slot) -> Option<T>,
    ) -> Option<T> {
        let slots = self.slots.lock().await;
        slots
            .get(name)
            .filter(|s| Arc::ptr_eq(&s.client, c))
            .and_then(f)
    }

    async fn store(&self, name: &str, c: &Arc<Client>, f: impl FnOnce(&mut Slot)) {
        if let Some(s) = self
            .slots
            .lock()
            .await
            .get_mut(name)
            .filter(|s| Arc::ptr_eq(&s.client, c))
        {
            f(s);
        }
    }

    /// `<saves_root>/<package>/<save>`, created.
    pub fn save_dir(&self, package: &str, save: &str) -> PathBuf {
        let d = self.saves_root.join(package).join(save);
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// Removes `<saves_root>/<package>/<save>` and what is in it, once the part has dropped the
    /// save, read-only folders too (a failure is logged). Ids that are not valid save ids are
    /// ignored.
    pub fn remove_save_dir(&self, package: &str, save: &str) {
        if self.registry.get(package).is_some() && valid_id(save) {
            remove_tree_or_log(&self.saves_root.join(package).join(save));
        }
    }

    /// Bytes on disk under the save storage.
    pub fn saves_bytes(&self) -> u64 {
        dir_bytes(&self.saves_root)
    }

    /// Bytes on disk under `package`'s save folders.
    pub fn package_saves_bytes(&self, package: &str) -> u64 {
        dir_bytes(&self.saves_root.join(package))
    }

    /// The names of `package`'s save folders (`<saves_root>/<package>/<save>`) that are save ids.
    pub fn save_dirs(&self, package: &str) -> Vec<String> {
        let Ok(rd) = std::fs::read_dir(self.saves_root.join(package)) else {
            return vec![];
        };
        let mut out: Vec<String> = rd
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| valid_id(n))
            .collect();
        out.sort();
        out
    }
}

/// Bytes of the files under `p`, recursively (0 when it doesn't exist). A file with several hard
/// links (saves that share unchanged files) is counted once.
fn dir_bytes(p: &Path) -> u64 {
    dir_bytes_seen(p, &mut std::collections::HashSet::new())
}

/// `dir_bytes`, skipping files whose `(device, inode)` is in `seen` (and adding the ones it counts).
fn dir_bytes_seen(p: &Path, seen: &mut std::collections::HashSet<(u64, u64)>) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let Ok(rd) = std::fs::read_dir(p) else {
        return 0;
    };
    let mut bytes = 0;
    for e in rd.filter_map(Result::ok) {
        bytes += match e.metadata() {
            Ok(m) if m.is_dir() => dir_bytes_seen(&e.path(), seen),
            Ok(m) if m.nlink() > 1 && !seen.insert((m.dev(), m.ino())) => 0,
            Ok(m) => m.len(),
            Err(_) => 0,
        };
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_links_are_counted_once() {
        let dir = std::env::temp_dir().join(format!("arena_hub_links_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("s1")).unwrap();
        std::fs::create_dir_all(dir.join("s2")).unwrap();
        std::fs::write(dir.join("s1/f"), vec![b'x'; 1000]).unwrap();
        std::fs::hard_link(dir.join("s1/f"), dir.join("s2/f")).unwrap();
        std::fs::write(dir.join("s2/g"), vec![b'y'; 10]).unwrap();
        let bytes = dir_bytes(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(bytes, 1010);
    }
}
