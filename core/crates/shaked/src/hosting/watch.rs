//! A sandbox's watcher: every poll it looks at the sandbox's branch, and kills the sandbox once
//! the branch no longer runs. A branch that can't be read is looked at again next time (the
//! sandbox's own timeout is the backstop).

use std::{sync::Arc, time::Duration};

use super::{ProviderRef, SandboxBackend};
use crate::{app::AppState, auth::Caller, model::BranchStatus, store::BranchId};

/// What one sandbox's watcher looks after.
pub(super) struct Watch {
    pub backend: Arc<dyn SandboxBackend>,
    pub sb: ProviderRef,
    pub branch: BranchId,
    /// Who the branch records for (its records are read as them).
    pub caller: Arc<Caller>,
    pub poll: Duration,
}

/// Watches `w`'s sandbox on its own task until it is killed.
pub(super) fn spawn(state: Arc<AppState>, w: Watch) {
    tokio::spawn(async move { w.run(&state).await });
}

impl Watch {
    async fn run(self, state: &AppState) {
        loop {
            tokio::time::sleep(self.poll).await;
            if self.over(state).await {
                break;
            }
        }
        kill(&*self.backend, &self.sb).await;
    }

    /// True once the branch has ended.
    async fn over(&self, state: &AppState) -> bool {
        match state.store.at(&self.caller).branch(&self.branch).await {
            Ok(b) => b.status != BranchStatus::Running,
            Err(e) => {
                eprintln!("hosting: reading branch {}: {e:#}", self.branch);
                false
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
