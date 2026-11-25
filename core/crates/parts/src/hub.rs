//! Lazily started part programs, one per package, shared by every branch.
use crate::client::{Client, ProgramDirs};
use crate::manifest::{Manifest, Registry};
use crate::request;
use anyhow::{anyhow, Result};
use shakedown_part::helpers::remove_tree_or_log;
use shakedown_part::proto::{valid_id, BenchDescribe, Describe};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, RwLockReadGuard};
use tokio::sync::Mutex;

struct Slot {
    client: Arc<Client>,
    describe: Option<Describe>,
    bench: Option<BenchDescribe>,
}

pub struct Hub {
    /// Written only to add a package installed while shaked runs.
    registry: RwLock<Registry>,
    bin_dir: PathBuf,
    /// Substituted for `${SHAKEDOWN_DIR}` in commands (left as it is when empty).
    repo_dir: PathBuf,
    saves_root: PathBuf,
    /// Extra environment for command-mode programs (e.g. `SHAKEDOWN_PG_URL`).
    env: Vec<(String, String)>,
    slots: Mutex<HashMap<String, Slot>>,
    /// Package programs start here (else `client` refuses, and benchmarks are described from
    /// their description files).
    programs: bool,
}

impl Hub {
    pub fn new(registry: Registry, bin_dir: PathBuf, saves_root: PathBuf) -> Hub {
        Hub {
            registry: RwLock::new(registry),
            bin_dir,
            repo_dir: PathBuf::new(),
            saves_root,
            env: vec![],
            slots: Mutex::default(),
            programs: true,
        }
    }

    /// Whether package programs start here. A hub without them never starts one: `client`
    /// refuses, and a benchmark is described from its description file ([`crate::described`]).
    /// For a shaked whose packages run only in sandboxes.
    pub fn with_programs(mut self, programs: bool) -> Hub {
        self.programs = programs;
        self
    }

    /// Whether package programs start here ([`with_programs`](Self::with_programs)).
    pub fn runs_programs(&self) -> bool {
        self.programs
    }

    /// Sets environment variables every command-mode program gets, over the passed-through ones.
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Hub {
        self.env = env;
        self
    }

    /// The repository folder `${SHAKEDOWN_DIR}` names in package commands.
    pub fn with_repo_dir(mut self, dir: PathBuf) -> Hub {
        self.repo_dir = dir;
        self
    }

    /// The installed packages. Never hold the guard across an `.await`.
    pub fn registry(&self) -> RwLockReadGuard<'_, Registry> {
        self.registry.read().unwrap()
    }

    /// Installs the package in `dir` (its `shakedown-package.toml`) while shaked runs; refuses a
    /// name that is installed already.
    pub fn install(&self, dir: &Path) -> Result<Manifest> {
        let m = Manifest::load(dir)?;
        self.registry.write().unwrap().add(m.clone())?;
        Ok(m)
    }

    fn dirs(&self) -> ProgramDirs<'_> {
        ProgramDirs {
            bin: &self.bin_dir,
            repo: &self.repo_dir,
            saves: &self.saves_root,
        }
    }

    /// The program of a package: started on first use, started again when it has stopped.
    pub async fn client(&self, name: &str) -> Result<Arc<Client>> {
        if !self.programs {
            return Err(anyhow!(
                "{name}: this shaked runs packages only in sandboxes"
            ));
        }
        let mut slots = self.slots.lock().await;
        if let Some(s) = slots.get(name) {
            if !s.client.is_stopped() {
                return Ok(s.client.clone());
            }
        }
        let m = (self.registry().get(name).cloned())
            .ok_or_else(|| anyhow!("no package named {name}"))?;
        let client = Arc::new(Client::spawn_in(&m, self.dirs(), &self.env).await?);
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

    /// `describe` of a benchmark, cached until the program is respawned; from its description
    /// file where programs don't start here.
    pub async fn bench_describe(&self, name: &str) -> Result<BenchDescribe> {
        if !self.programs {
            return self.described(name);
        }
        let client = self.client(name).await?;
        if let Some(d) = self.cached(name, &client, |s| s.bench.clone()).await {
            return Ok(d);
        }
        let d: BenchDescribe = client.request(&request::Describe {}).await?;
        self.store(name, &client, |s| s.bench = Some(d.clone()))
            .await;
        Ok(d)
    }

    /// The description file of the benchmark `name`.
    fn described(&self, name: &str) -> Result<BenchDescribe> {
        let m = (self.registry().get(name).cloned())
            .ok_or_else(|| anyhow!("no package named {name}"))?;
        crate::described::read(&m)
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

    /// The folder every package's saves are kept under.
    pub fn saves_root(&self) -> &Path {
        &self.saves_root
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
        if self.registry().get(package).is_some() && valid_id(save) {
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
        let dir = std::env::temp_dir().join(format!("shakedown_hub_links_{}", std::process::id()));
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

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shakedown_hub_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A benchmark package `name` in a fresh folder of its own.
    fn pkg_dir(tag: &str, name: &str) -> PathBuf {
        let dir = temp(tag).join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let toml = format!(
            "kind = \"benchmark\"\nname = \"{name}\"\nversion = \"0.1.0\"\nprotocol = 1\nrun = {{ command = [\"x\"] }}\n"
        );
        std::fs::write(dir.join("shakedown-package.toml"), toml).unwrap();
        dir
    }

    #[tokio::test]
    async fn a_package_installed_at_runtime_is_found_and_a_taken_name_is_refused() {
        let empty = temp("empty");
        let registry = Registry::scan(std::slice::from_ref(&empty)).unwrap();
        let hub = Hub::new(registry, empty.clone(), empty.clone());
        let (first, second) = (pkg_dir("one", "acme"), pkg_dir("two", "acme"));
        assert_eq!(hub.install(&first).unwrap().dir, first);
        assert!(hub.registry().get("acme").is_some());
        let e = hub.install(&second).unwrap_err();
        assert_eq!(e.to_string(), "a package named acme is installed already");
        assert_eq!(hub.registry().get("acme").unwrap().dir, first);
        let _ = std::fs::remove_dir_all(&empty);
        for d in [first, second] {
            let _ = std::fs::remove_dir_all(d.parent().unwrap()); // the package's own temp folder
        }
    }
}
