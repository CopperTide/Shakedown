//! Sandboxes for hosted runs: what a branch's sandbox is asked for, and the backends that make
//! one. A backend creates a sandbox, writes a file into it (the worker's ticket), kills it and
//! says whether it still runs. `Hosting`, which starts a branch in one, and the fake backend
//! come with M2a Task 7; the E2B backend is `e2b`.

pub mod e2b;

use std::{collections::BTreeMap, time::Duration};

use futures_util::future::BoxFuture;

/// Where the control box writes a sandbox's ticket; `shaked worker --ticket-file` waits on it.
pub const TICKET_PATH: &str = "/run/shaked/ticket";

/// What a branch's sandbox is created with.
#[derive(Debug, Clone, PartialEq)]
pub struct SandboxSpec {
    /// The backend's template (image) to boot.
    pub template: String,
    /// The only hosts the sandbox may reach: the control box's link host and the hosts the agent
    /// setup allows. Everything else is denied.
    pub allow_out: Vec<String>,
    /// The sandbox is killed by its backend after this long, whatever happens to the control box.
    pub timeout: Duration,
    /// Labels kept with the sandbox (who it runs for, its run and branch). Never secrets.
    pub metadata: BTreeMap<String, String>,
}

/// The backend's name for a sandbox (E2B's `sandboxID`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderRef(pub String);

/// Whether a sandbox still runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderState {
    Running,
    /// Killed, timed out, paused (Shakedown never pauses one) or never there.
    Gone,
}

/// A place sandboxes are made.
pub trait SandboxBackend: Send + Sync {
    fn create<'a>(&'a self, spec: &'a SandboxSpec) -> BoxFuture<'a, anyhow::Result<ProviderRef>>;
    /// Writes `bytes` to `path` in the sandbox, making its folders.
    fn write_file<'a>(
        &'a self,
        sb: &'a ProviderRef,
        path: &'a str,
        bytes: Vec<u8>,
    ) -> BoxFuture<'a, anyhow::Result<()>>;
    /// Kills the sandbox. A sandbox already gone counts as killed.
    fn kill<'a>(&'a self, sb: &'a ProviderRef) -> BoxFuture<'a, anyhow::Result<()>>;
    fn state<'a>(&'a self, sb: &'a ProviderRef) -> BoxFuture<'a, anyhow::Result<ProviderState>>;
}
