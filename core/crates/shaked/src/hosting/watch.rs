//! A sandbox's watcher: every poll it looks at the sandbox's branch and at the sandbox. It kills
//! the sandbox once the branch no longer runs, ends the branch `stopped` ([`LONGEST_REACHED`])
//! at its deadline, and `infra_error` ([`SANDBOX_STOPPED`]) when the sandbox vanished under it.
//! It holds its scope's slot until the sandbox is killed. What can't be read is looked at again
//! next time (the sweep and the sandbox's own timeout are the backstops).

use std::{sync::Arc, time::Duration};

use super::{
    end_branch, ProviderRef, ProviderState, SandboxBackend, Slot, LONGEST_REACHED, SANDBOX_STOPPED,
};
use crate::{app::AppState, auth::Caller, model::BranchStatus, store::BranchId};

/// What one sandbox's watcher looks after.
pub(super) struct Watch {
    pub backend: Arc<dyn SandboxBackend>,
    pub sb: ProviderRef,
    pub branch: BranchId,
    /// Who the branch records for (its records are read as them).
    pub caller: Arc<Caller>,
    pub poll: Duration,
    /// When the branch has gone the longest a run may.
    pub deadline: tokio::time::Instant,
    /// Its scope's slot, given back when the watcher ends.
    pub _slot: Slot,
}

/// What a watcher does next.
enum Next {
    Watch,
    /// The branch has ended: kill the sandbox.
    Kill,
    /// End the branch so (then kill the sandbox).
    End(BranchStatus, &'static str),
}

/// Watches `w`'s sandbox on its own task until it is killed.
pub(super) fn spawn(state: Arc<AppState>, w: Watch) {
    tokio::spawn(async move { w.run(&state).await });
}

impl Watch {
    async fn run(self, state: &AppState) {
        loop {
            tokio::time::sleep(self.poll).await;
            match self.next(state).await {
                Next::Watch => continue,
                Next::Kill => break,
                Next::End(status, why) => {
                    end_branch(state, &self.branch, status, why).await;
                    break;
                }
            }
        }
        kill(&*self.backend, &self.sb).await;
    }

    async fn next(&self, state: &AppState) -> Next {
        if !self.running(state).await {
            return Next::Kill;
        }
        if tokio::time::Instant::now() >= self.deadline {
            return Next::End(BranchStatus::Stopped, LONGEST_REACHED);
        }
        match self.backend.state(&self.sb).await {
            Ok(ProviderState::Gone) => Next::End(BranchStatus::InfraError, SANDBOX_STOPPED),
            _ => Next::Watch,
        }
    }

    /// False once the branch has ended.
    async fn running(&self, state: &AppState) -> bool {
        match state.store.at(&self.caller).branch(&self.branch).await {
            Ok(b) => b.status == BranchStatus::Running,
            Err(e) => {
                eprintln!("hosting: reading branch {}: {e:#}", self.branch);
                true
            }
        }
    }
}

/// Kills `sb`, logging a failure (its own timeout ends it then).
pub(super) async fn kill(backend: &dyn SandboxBackend, sb: &ProviderRef) {
    if let Err(e) = backend.kill(sb).await {
        eprintln!("hosting: killing sandbox {}: {e:#}", sb.0);
    }
}
