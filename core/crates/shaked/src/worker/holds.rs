//! Pausing, resuming and stopping a branch a sandbox's worker runs, with the control box as the
//! authority. A hold is kept in the store first (a restart keeps it), then on the branch's link,
//! both under the branch's lock ([`keep`]), then told to the worker when one is connected
//! ([`tell`]), and told again whenever it (re)joins. The gateway enforces it on the branch's model
//! calls whether a worker is connected or not. A resume extends the worker token by how long the
//! pause lasted (the store keeps when it began).
//!
//! A stop ends the branch on the control box: at once when no worker is connected, else with the
//! hold's status when the worker's `Ended` comes, or after [`Config::link_end_grace`] when it
//! doesn't. Every end holds the branch's lock over the status and the revoke, so no worker event
//! lands after it. The grace is armed again after a restart: by [`sweep`] at start, and when a
//! rejoin rebuilds the link of a stopping branch. The sweep, at start and every
//! [`Config::link_sweep_every`], also ends `infra_error` a running, unpaused branch whose worker
//! token died (it expired, a day after it was made): no worker can come back to it.
//!
//! [`Config::link_end_grace`]: crate::app::Config
//! [`Config::link_sweep_every`]: crate::app::Config

use std::sync::Arc;

use anyhow::Result;

use super::{frames::ToWorker, links::frame_of};
use crate::{
    app::AppState,
    auth::Caller,
    jobs::EndIf,
    model::{BranchStatus, Hold},
};

/// What a branch whose worker token died ends with.
pub const TOKEN_DIED: &str = "the sandbox's worker token expired before the branch ended";

/// The error a linked branch ends with for its hold: the limit's reason for one stopped for its
/// spend limit, none for a stop pressed by a person.
pub fn stop_error(status: BranchStatus, why: &str) -> Option<String> {
    (status == BranchStatus::BudgetExceeded).then(|| why.to_string())
}

/// Puts linked `branch` on `hold` (for `why`, a stop for the spend limit's reason), recorded for
/// `caller`, and tells its worker ([`keep`], then [`tell`]): whether the hold changed.
pub async fn put(
    state: &Arc<AppState>,
    caller: &Caller,
    branch: &str,
    hold: Hold,
    why: &str,
) -> Result<bool> {
    let (changed, now) = keep(state, caller, branch, hold).await?;
    tell(state, branch, (hold, now), (changed, why)).await?;
    Ok(changed)
}

/// Keeps `hold` in the store, then on the link, under the branch's lock: whether it changed, and
/// the hold now. A resume extends the branch's tokens by the time the pause lasted (the store does
/// both at once).
pub async fn keep(
    state: &AppState,
    caller: &Caller,
    branch: &str,
    hold: Hold,
) -> Result<(bool, Hold)> {
    let lock = state.store.branch_lock(branch);
    let _locked = lock.lock().await;
    let at = state.store.at(caller);
    if !at.set_hold(branch, hold).await? {
        return Ok((false, at.hold(branch).await?));
    }
    state.links.set_hold(branch, hold);
    Ok((true, hold))
}

/// Tells `branch`'s worker the hold `now` that `asked` came to (`changed` when it did; `why`, a
/// stop for the spend limit's reason). A stop ends the branch at once when no worker is connected,
/// else arms its end after the grace; a stop pressed again does so again.
pub async fn tell(
    state: &Arc<AppState>,
    branch: &str,
    (asked, now): (Hold, Hold),
    (changed, why): (bool, &str),
) -> Result<()> {
    let stop = now.ending().filter(|_| asked.ending().is_some());
    if !changed && stop.is_none() {
        return Ok(());
    }
    // A stop pressed on a branch stopping for its limit keeps the limit's reason.
    let over = stop == Some(BranchStatus::BudgetExceeded) && asked != now;
    let why = match over {
        true => state.links.limit_reason(branch),
        false => why.to_string(),
    };
    let resume = (now == Hold::Running).then_some(ToWorker::Resume);
    let frame = frame_of(now, &why).or(resume);
    let sent = frame.is_some_and(|f| state.links.send(branch, f));
    match stop {
        Some(status) if sent => arm(state, branch, status, &why),
        Some(status) => {
            let ending = (status, stop_error(status, &why));
            end(state, branch, ending, EndIf::Running).await?;
        }
        None => {}
    }
    Ok(())
}

/// Arms `branch`'s end with `status` after the grace, unless it is armed already.
pub fn arm(state: &Arc<AppState>, branch: &str, status: BranchStatus, why: &str) {
    if !state.links.arm(branch) {
        return;
    }
    let (state, branch, error) = (state.clone(), branch.to_string(), stop_error(status, why));
    tokio::spawn(async move {
        tokio::time::sleep(state.config.link_end_grace).await;
        if let Err(e) = end(&state, &branch, (status, error), EndIf::Running).await {
            eprintln!("worker link {branch}: ending it after the grace: {e:#}");
        }
        state.links.disarm(&branch);
    });
}

/// Ends linked `branch` with `status` (and `error`) when it is still running (and `when` says so),
/// under its lock ([`end_locked`]).
pub async fn end(
    state: &AppState,
    branch: &str,
    (status, error): (BranchStatus, Option<String>),
    when: EndIf,
) -> Result<()> {
    let lock = state.store.branch_lock(branch);
    let _locked = lock.lock().await;
    end_locked(state, branch, (status, error), when).await
}

/// [`end`], for a caller that holds the branch's lock already: the branch ends with `status`
/// when it still runs, its tokens are revoked (its worker's calls are refused from then on) and
/// its link forgotten, closing a connection still open with the status the branch ended with.
pub async fn end_locked(
    state: &AppState,
    branch: &str,
    (status, error): (BranchStatus, Option<String>),
    when: EndIf,
) -> Result<()> {
    let ended = crate::jobs::end_linked(&state.store, branch, (status, error), when).await?;
    match ended {
        Some(BranchStatus::Running) => {}
        ended => state.links.forget(branch, ended.unwrap_or(status)),
    }
    Ok(())
}

/// Ends or arms what the control box must for every running linked branch: the grace of one it
/// stops (a restart lost the timer), the end of one whose worker token died. A paused branch is
/// left alone (its resume extends the token). A branch that can't be ended is logged, and the
/// sweep goes on.
pub async fn sweep(state: &Arc<AppState>) -> Result<()> {
    for b in crate::jobs::linked_branches(&state.store).await? {
        if b.hold == Hold::Paused {
            continue;
        }
        if let Some(status) = b.hold.ending() {
            arm(
                state,
                &b.branch,
                status,
                &state.links.limit_reason(&b.branch),
            );
        } else if !b.token_live {
            let died = (BranchStatus::InfraError, Some(TOKEN_DIED.to_string()));
            let ended = end(state, &b.branch, died, EndIf::TokenDied).await;
            if let Err(e) = ended {
                eprintln!("worker links: ending {}: {e:#}", b.branch);
            }
        }
    }
    Ok(())
}

/// [`sweep`] at start, then every [`Config::link_sweep_every`](crate::app::Config).
pub async fn sweep_every(state: Arc<AppState>) {
    loop {
        if let Err(e) = sweep(&state).await {
            eprintln!("worker links: sweeping: {e:#}");
        }
        tokio::time::sleep(state.config.link_sweep_every).await;
    }
}
