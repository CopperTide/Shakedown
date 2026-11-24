//! Sandboxes for hosted runs. A control box with a [`Hosting`] runs each new branch in a sandbox of
//! its own: it records the branch, creates the sandbox, hands its `shaked worker` a ticket at
//! [`TICKET_PATH`], and kills the sandbox once the branch has ended ([`watch`]); a branch whose
//! sandbox vanished ends `infra_error`, one past the longest run ends `stopped`. A scope (the
//! layer's `object_prefix`) runs at most [`PER_SCOPE`] sandboxes at a time ([`slots`]). Every minute a [`sweep`] kills this control box's
//! sandboxes whose branch no longer runs, or that outlived their timeout (a watcher a restart
//! lost, a create whose answer was lost). A backend creates a sandbox, writes a file into it,
//! kills it, says whether it still runs and lists sandboxes by label: E2B (`e2b`), or processes
//! on this machine for tests (`fake`).

pub mod e2b;
#[cfg(any(test, feature = "harness"))]
pub mod fake;
mod slots;
mod sweep;
mod watch;
mod worlds;

pub use slots::{Slot, BUSY, PER_SCOPE};
pub use sweep::{sweep, sweep_every};
pub use worlds::SANDBOX_WORLDS;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use anyhow::Context;
use futures_util::future::BoxFuture;
use uuid::Uuid;

use crate::{
    app::AppState, auth::Caller, jobs::EndIf, layer::Refusal, model::BranchStatus,
    runner::BranchPlan, worker::Ticket,
};

/// Where the control box writes a sandbox's ticket; `shaked worker --ticket-file` waits on it.
pub const TICKET_PATH: &str = "/run/shaked/ticket";

/// The longest a sandbox lives, whatever a run asks: E2B Hobby's continuous runtime.
pub const SANDBOX_CAP: Duration = Duration::from_secs(60 * 60);
/// How long a sandbox outlives its run's cap, so its worker can say how the branch ended.
pub const END_GRACE: Duration = Duration::from_secs(5 * 60);
/// The longest a hosted run goes (and the default cap): the sandbox's cap less the grace.
pub const LONGEST_RUN: Duration = Duration::from_secs(55 * 60);
/// How often a sandbox's watcher looks at its branch.
const POLL: Duration = Duration::from_secs(5);

/// The label naming the control box a sandbox runs for (its link host).
pub const CONTROL_LABEL: &str = "shaked";

/// What a branch whose sandbox could not start ends with (the cause is logged, not shown).
pub const NO_SANDBOX: &str = "its sandbox could not start";
/// What a branch whose sandbox vanished (killed, timed out, crashed) ends with.
pub const SANDBOX_STOPPED: &str = "its sandbox stopped";
/// What a branch stopped at the longest run ends with.
pub const LONGEST_REACHED: &str = "stopped: the longest a cloud run may go was reached";

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

/// A running sandbox and its labels.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub sb: ProviderRef,
    pub metadata: BTreeMap<String, String>,
}

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
    /// Writes `bytes` to `path` in the sandbox, making its folders. The file appears whole: it is
    /// written to [`partial_of`] `path` and renamed onto `path`.
    fn write_file<'a>(
        &'a self,
        sb: &'a ProviderRef,
        path: &'a str,
        bytes: Vec<u8>,
    ) -> BoxFuture<'a, anyhow::Result<()>>;
    /// Kills the sandbox. A sandbox already gone counts as killed.
    fn kill<'a>(&'a self, sb: &'a ProviderRef) -> BoxFuture<'a, anyhow::Result<()>>;
    fn state<'a>(&'a self, sb: &'a ProviderRef) -> BoxFuture<'a, anyhow::Result<ProviderState>>;
    /// The running sandboxes labelled with every one of `labels`.
    fn list<'a>(
        &'a self,
        labels: &'a BTreeMap<String, String>,
    ) -> BoxFuture<'a, anyhow::Result<Vec<Listed>>>;
}

/// Where a file bound for `path` is written before it is renamed onto `path`.
pub fn partial_of(path: &str) -> String {
    format!("{path}.partial")
}

/// Runs new branches in sandboxes from `backend`.
pub struct Hosting {
    backend: Arc<dyn SandboxBackend>,
    /// The control box's link host: the one host a sandbox reaches, and its [`CONTROL_LABEL`].
    link_host: String,
    template: String,
    longest_run: Duration,
    poll: Duration,
    slots: slots::Slots,
}

impl std::fmt::Debug for Hosting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hosting")
            .field("link_host", &self.link_host)
            .field("template", &self.template)
            .field("longest_run", &self.longest_run)
            .finish_non_exhaustive()
    }
}

impl Hosting {
    /// Sandboxes from `backend`'s `template`, whose worker links to `link_url` (the template has
    /// it baked in; here it names the one host a sandbox may reach).
    pub fn new(
        backend: Arc<dyn SandboxBackend>,
        link_url: &str,
        template: &str,
    ) -> anyhow::Result<Hosting> {
        let url = reqwest::Url::parse(link_url).context("the worker link URL")?;
        let host = url
            .host_str()
            .context("the worker link URL names no host")?;
        Ok(Hosting {
            backend,
            link_host: host.to_string(),
            template: template.to_string(),
            longest_run: LONGEST_RUN,
            poll: POLL,
            slots: Default::default(),
        })
    }

    /// Runs end after `longest` at most (never past [`LONGEST_RUN`]).
    pub fn with_longest_run(self, longest: Duration) -> Hosting {
        let longest_run = longest.min(LONGEST_RUN);
        Hosting {
            longest_run,
            ..self
        }
    }

    /// Watchers look at their branch every `poll` (tests).
    pub fn with_poll(self, poll: Duration) -> Hosting {
        Hosting { poll, ..self }
    }

    /// A sandbox's timeout: the longest run plus [`END_GRACE`], never past [`SANDBOX_CAP`].
    pub fn sandbox_timeout(&self) -> Duration {
        (self.longest_run + END_GRACE).min(SANDBOX_CAP)
    }

    /// A sandbox slot of `caller`'s scope (the layer's `object_prefix`), refused when the scope
    /// runs [`PER_SCOPE`] already. Taken before the run is recorded.
    pub fn reserve(&self, state: &AppState, caller: &Caller) -> Result<Slot, Refusal> {
        self.slots.take(&state.store.layer().object_prefix(caller))
    }

    /// The label every sandbox of this control box has.
    fn ours(&self) -> BTreeMap<String, String> {
        BTreeMap::from([(CONTROL_LABEL.to_string(), self.link_host.clone())])
    }

    /// The sandbox for `plan`'s branch, labelled with this control box, `scope`, the run, the
    /// branch, the `sandbox` its worker token names and when it started.
    fn spec(&self, plan: &BranchPlan, scope: String, sandbox: Uuid) -> SandboxSpec {
        let started = chrono::Utc::now().timestamp().to_string();
        let mut metadata = self.ours();
        metadata.extend([
            ("scope".to_string(), scope),
            ("run".to_string(), plan.run_id.clone()),
            ("branch".to_string(), plan.branch_id.clone()),
            ("sandbox".to_string(), sandbox.to_string()),
            ("started".to_string(), started),
        ]);
        SandboxSpec {
            template: self.template.clone(),
            allow_out: vec![self.link_host.clone()],
            timeout: self.sandbox_timeout(),
            metadata,
        }
    }

    /// Runs `plan`'s branch (just recorded by `record_start`) for `caller` in a new sandbox, on
    /// `slot`: the link expects it, the sandbox is created, a world built here is copied into it
    /// ([`worlds`]), its worker's ticket written to [`TICKET_PATH`], and a watcher kills the sandbox once the branch ends (and gives the slot
    /// back). A sandbox that can't take its ticket is killed.
    pub async fn start(
        &self,
        state: &Arc<AppState>,
        plan: &BranchPlan,
        caller: &Arc<Caller>,
        slot: Slot,
    ) -> anyhow::Result<()> {
        let sandbox = Uuid::new_v4();
        let scope = state.store.layer().object_prefix(caller);
        let world = worlds::world_files(state, &plan.benchmark)?;
        state.links.expect(plan.clone());
        let spec = self.spec(plan, scope, sandbox);
        let sb = self.backend.create(&spec).await?;
        let ticket = Ticket {
            branch_id: plan.branch_id.clone(),
            sandbox,
            caller: caller.clone(),
        };
        if let Err(e) = self.ready(state, &sb, world, ticket).await {
            watch::kill(&*self.backend, &sb).await;
            return Err(e.context("handing the sandbox its world and ticket"));
        }
        watch::spawn(state.clone(), self.watch(sb, plan, caller, slot));
        Ok(())
    }

    /// Copies `world` into `sb`, then issues `ticket` and writes it, newline-ended, to the
    /// sandbox's [`TICKET_PATH`].
    async fn ready(
        &self,
        state: &AppState,
        sb: &ProviderRef,
        world: Vec<worlds::WorldFile>,
        ticket: Ticket,
    ) -> anyhow::Result<()> {
        worlds::copy(&*self.backend, sb, world).await?;
        let secret = state.links.tickets.issue(ticket);
        let bytes = format!("{secret}\n").into_bytes();
        self.backend.write_file(sb, TICKET_PATH, bytes).await
    }

    /// The watcher of `sb`, running `plan`'s branch for `caller` on `slot`, until the longest run.
    fn watch(
        &self,
        sb: ProviderRef,
        plan: &BranchPlan,
        c: &Arc<Caller>,
        slot: Slot,
    ) -> watch::Watch {
        watch::Watch {
            backend: self.backend.clone(),
            sb,
            branch: plan.branch_id.clone(),
            caller: c.clone(),
            poll: self.poll,
            deadline: tokio::time::Instant::now() + self.longest_run,
            _slot: slot,
        }
    }

    /// [`start`](Self::start), the branch ended `infra_error` ([`NO_SANDBOX`]) when it fails.
    pub async fn start_or_fail(
        &self,
        state: &Arc<AppState>,
        plan: &BranchPlan,
        c: &Arc<Caller>,
        slot: Slot,
    ) {
        let Err(e) = self.start(state, plan, c, slot).await else {
            return;
        };
        eprintln!("hosting: starting branch {}: {e:#}", plan.branch_id);
        end_branch(state, &plan.branch_id, BranchStatus::InfraError, NO_SANDBOX).await;
    }
}

/// Ends `branch` with `status` and `why` when it still runs (a failure is logged).
async fn end_branch(state: &AppState, branch: &str, status: BranchStatus, why: &str) {
    let ended = (status, Some(why.to_string()));
    let ended = crate::worker::holds::end(state, branch, ended, EndIf::Running).await;
    if let Err(e) = ended {
        eprintln!("hosting: ending branch {branch}: {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosting() -> Hosting {
        let fake = Arc::new(fake::FakeBackend::new("shaked", "ws://h/worker", &[]));
        Hosting::new(fake, "wss://sd.example/worker", "t").unwrap()
    }

    #[test]
    fn a_sandbox_lives_the_longest_run_and_a_grace_never_past_the_cap() {
        let h = hosting();
        assert_eq!(h.sandbox_timeout(), SANDBOX_CAP);
        let short = hosting().with_longest_run(Duration::from_secs(60));
        assert_eq!(short.sandbox_timeout(), Duration::from_secs(60) + END_GRACE);
        let long = hosting().with_longest_run(Duration::from_secs(24 * 3600));
        assert_eq!(long.sandbox_timeout(), SANDBOX_CAP);
        assert_eq!(h.ours()[CONTROL_LABEL], "sd.example");
    }
}
