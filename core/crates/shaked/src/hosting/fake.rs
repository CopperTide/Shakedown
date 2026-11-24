//! A sandbox backend on this machine, for tests. A "sandbox" is a folder of its own and a
//! `shaked worker` process linked to the control box, waiting on the folder's [`TICKET_PATH`];
//! its saves and worlds are kept in the folder. Killing it ends the process and removes the
//! folder. It counts the creates and kills it was asked for.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use futures_util::future::BoxFuture;
use tokio::process::{Child, Command};

use super::{partial_of, ProviderRef, ProviderState, SandboxBackend, SandboxSpec, TICKET_PATH};

/// Sandboxes as local `shaked worker` processes.
pub struct FakeBackend {
    /// The `shaked` binary.
    shaked: PathBuf,
    /// The control box's worker link (`ws://…/worker`).
    link: String,
    /// More environment for every worker.
    env: Vec<(String, String)>,
    sandboxes: Mutex<HashMap<String, Fake>>,
    created: AtomicUsize,
    killed: AtomicUsize,
}

/// One fake sandbox.
struct Fake {
    dir: PathBuf,
    worker: Child,
}

/// Where `path` in the sandbox whose folder is `dir` is on this machine.
fn local(dir: &Path, path: &str) -> PathBuf {
    dir.join(path.trim_start_matches('/'))
}

/// Writes `bytes` beside `path` and renames the file onto it, making its folders.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = PathBuf::from(partial_of(&path.to_string_lossy()));
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path)
}

impl FakeBackend {
    /// Workers run `shaked` (the binary), link to `link`, with more environment `env`.
    pub fn new(shaked: impl Into<PathBuf>, link: &str, env: &[(&str, &str)]) -> FakeBackend {
        let env = env.iter().map(|(k, v)| (k.to_string(), v.to_string()));
        FakeBackend {
            shaked: shaked.into(),
            link: link.to_string(),
            env: env.collect(),
            sandboxes: Mutex::default(),
            created: AtomicUsize::new(0),
            killed: AtomicUsize::new(0),
        }
    }

    /// How many sandboxes it was asked to create.
    pub fn created(&self) -> usize {
        self.created.load(Ordering::SeqCst)
    }

    /// How many kills it was asked for.
    pub fn killed(&self) -> usize {
        self.killed.load(Ordering::SeqCst)
    }

    /// A worker for the sandbox in `dir`.
    fn worker(&self, dir: &Path) -> std::io::Result<Child> {
        Command::new(&self.shaked)
            .args(["worker", "--link", &self.link, "--ticket-file"])
            .arg(local(dir, TICKET_PATH))
            .env("SHAKEDOWN_SAVES", dir.join("saves"))
            .env("SHAKEDOWN_WORLDS", dir.join("worlds"))
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .kill_on_drop(true)
            .spawn()
    }

    fn dir_of(&self, sb: &ProviderRef) -> anyhow::Result<PathBuf> {
        let sandboxes = self.sandboxes.lock().unwrap();
        let found = sandboxes.get(&sb.0).map(|f| f.dir.clone());
        found.ok_or_else(|| anyhow::anyhow!("no sandbox {}", sb.0))
    }
}

impl SandboxBackend for FakeBackend {
    fn create<'a>(&'a self, _spec: &'a SandboxSpec) -> BoxFuture<'a, anyhow::Result<ProviderRef>> {
        Box::pin(async move {
            self.created.fetch_add(1, Ordering::SeqCst);
            let id = format!("fake-{}", uuid::Uuid::new_v4().simple());
            let dir = std::env::temp_dir().join(format!("shaked-{id}"));
            std::fs::create_dir_all(&dir)?;
            let worker = self.worker(&dir)?;
            let fake = Fake { dir, worker };
            self.sandboxes.lock().unwrap().insert(id.clone(), fake);
            Ok(ProviderRef(id))
        })
    }

    fn write_file<'a>(
        &'a self,
        sb: &'a ProviderRef,
        path: &'a str,
        bytes: Vec<u8>,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move { Ok(write_whole(&local(&self.dir_of(sb)?, path), &bytes)?) })
    }

    fn kill<'a>(&'a self, sb: &'a ProviderRef) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move {
            self.killed.fetch_add(1, Ordering::SeqCst);
            let Some(mut fake) = self.sandboxes.lock().unwrap().remove(&sb.0) else {
                return Ok(());
            };
            // A worker that has exited already counts as killed.
            fake.worker.kill().await.ok();
            Ok(std::fs::remove_dir_all(&fake.dir)?)
        })
    }

    fn state<'a>(&'a self, sb: &'a ProviderRef) -> BoxFuture<'a, anyhow::Result<ProviderState>> {
        Box::pin(async move {
            let mut sandboxes = self.sandboxes.lock().unwrap();
            let running = match sandboxes.get_mut(&sb.0) {
                Some(f) => f.worker.try_wait()?.is_none(),
                None => false,
            };
            Ok(match running {
                true => ProviderState::Running,
                false => ProviderState::Gone,
            })
        })
    }
}
