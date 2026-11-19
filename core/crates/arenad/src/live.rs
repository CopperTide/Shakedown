//! The branches running in this arenad, by branch id.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::{app::AppState, exec::BranchCtx, runner};

/// Branches currently running, by branch id. A branch is in it from the moment it is live (its
/// world started) until its world has stopped.
#[derive(Default)]
pub struct LiveBranches(RwLock<HashMap<String, Arc<BranchCtx>>>);

impl LiveBranches {
    pub fn get(&self, branch: &str) -> Option<Arc<BranchCtx>> {
        self.0.read().unwrap().get(branch).cloned()
    }

    pub fn contains(&self, branch: &str) -> bool {
        self.0.read().unwrap().contains_key(branch)
    }

    pub fn is_empty(&self) -> bool {
        self.0.read().unwrap().is_empty()
    }

    /// Makes `ctx` live, under its branch id.
    pub fn insert(&self, ctx: Arc<BranchCtx>) {
        let id = ctx.branch_id.clone();
        self.0.write().unwrap().insert(id, ctx);
    }

    pub fn remove(&self, branch: &str) {
        self.0.write().unwrap().remove(branch);
    }

    /// The live `branch`, when it has `agent` (one of its team, or one that joined).
    pub fn agent(&self, branch: &str, agent: &str) -> Option<Arc<BranchCtx>> {
        self.get(branch).filter(|c| c.roster.has_agent(agent))
    }

    /// Stops `branch` if it is live here, as `POST /api/branches/{id}/stop` does.
    pub async fn stop(&self, state: &AppState, branch: &str) {
        if let Some(ctx) = self.get(branch) {
            runner::stop(state, &ctx).await;
        }
    }

    /// Ends `branch` `failed` with `why` (scored) if it is live here; false when it isn't.
    pub async fn fail_if_live(&self, state: &AppState, branch: &str, why: String) -> bool {
        let Some(ctx) = self.get(branch) else {
            return false;
        };
        runner::finish_failed(state, &ctx, why).await;
        true
    }
}
