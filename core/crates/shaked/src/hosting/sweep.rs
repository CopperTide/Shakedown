//! The sandbox sweep, the backstop for what watchers miss: a watcher a control-box restart lost,
//! a sandbox whose create answered too late to be watched. It lists this control box's
//! sandboxes (labelled [`CONTROL_LABEL`] with its link host) and kills each one whose branch no
//! longer runs, or that is older than its timeout; a branch still running on a sandbox it kills
//! ends `infra_error` ([`SANDBOX_STOPPED`]).

use std::{collections::HashSet, sync::Arc};

use anyhow::Result;

use super::{end_branch, watch::kill, Hosting, Listed, SANDBOX_STOPPED};
use crate::{app::AppState, model::BranchStatus};

/// Why a sandbox is killed.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Keep,
    /// Its branch has ended, or it names none.
    Ended,
    /// It is older than its timeout, its branch still running.
    TooOld,
}

/// The verdict on `l`, `running` the branches that still run, `now` in unix seconds.
fn verdict(hosting: &Hosting, l: &Listed, running: &HashSet<String>, now: i64) -> Verdict {
    let Some(branch) = l.metadata.get("branch") else {
        return Verdict::Ended;
    };
    if !running.contains(branch) {
        return Verdict::Ended;
    }
    let started = l
        .metadata
        .get("started")
        .and_then(|s| s.parse::<i64>().ok());
    let limit = hosting.sandbox_timeout().as_secs() as i64;
    match started {
        Some(t) if now - t > limit => Verdict::TooOld,
        _ => Verdict::Keep,
    }
}

/// Sweeps the sandboxes of `state`'s [`Hosting`] once (nothing without one).
pub async fn sweep(state: &Arc<AppState>) -> Result<()> {
    let Some(hosting) = state.hosting.get() else {
        return Ok(());
    };
    let ours = hosting.backend.list(&hosting.ours()).await?;
    let branches: Vec<String> = ours
        .iter()
        .filter_map(|l| l.metadata.get("branch").cloned())
        .collect();
    let running = crate::jobs::running_among(&state.store, &branches).await?;
    let now = chrono::Utc::now().timestamp();
    for l in &ours {
        let v = verdict(hosting, l, &running, now);
        if v == Verdict::TooOld {
            let branch = &l.metadata["branch"];
            end_branch(state, branch, BranchStatus::InfraError, SANDBOX_STOPPED).await;
        }
        if v != Verdict::Keep {
            kill(&*hosting.backend, &l.sb).await;
        }
    }
    Ok(())
}

/// [`sweep`] at start, then every [`Config::link_sweep_every`](crate::app::Config).
pub async fn sweep_every(state: Arc<AppState>) {
    loop {
        if let Err(e) = sweep(&state).await {
            eprintln!("hosting: sweeping sandboxes: {e:#}");
        }
        tokio::time::sleep(state.config.link_sweep_every).await;
    }
}
