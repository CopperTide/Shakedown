//! A run's world time window (`world_minutes`): a run an external system drives is scored when
//! its branch clock reaches the window's end, however its driver fares. A paused clock stands
//! still, so a pause extends the window. A driver that finishes before the end is held: its
//! agents are ended, and the run goes on until the window ends, then is scored as `finish` (or
//! `finish_failed` when the held finish carried an error) would. The run's ending stays with
//! `runner::finish` and `runner::finish_failed`.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use crate::{app::AppState, exec::BranchCtx, model::*, runner};

/// The longest window: a day of world time.
pub const MAX_MINUTES: f64 = 1440.0;
/// How often the watch looks at the clock.
const POLL: Duration = Duration::from_millis(100);
pub const INVALID: &str = "world_minutes must be from 0 to 1440 minutes of world time";

/// A finish a driver asked for before its run's window ended: the error it carried, if any.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Held {
    pub error: Option<String>,
}

/// True for a window's length: more than 0 and at most `MAX_MINUTES`.
pub fn valid_minutes(m: f64) -> bool {
    m.is_finite() && m > 0.0 && m <= MAX_MINUTES
}

/// The branch clock (ms) at which the window of `rules` ends; None without one.
pub fn limit_ms(rules: &Rules) -> Option<i64> {
    rules.world_minutes.map(|m| (m * 60_000.0).round() as i64)
}

/// Watches the live external branch `branch` until its window ends, when it has one.
pub fn watch(state: &Arc<AppState>, branch: &str) {
    let Some(ctx) = state.live.get(branch) else {
        return;
    };
    if let (Runner::External, Some(limit)) = (ctx.rules.runner, limit_ms(&ctx.rules)) {
        spawn(state.clone(), ctx, limit);
    }
}

/// Waits until `ctx`'s clock reaches `limit_ms`, then records `window/time_up` and finishes the
/// branch. The watch ends with the branch: a branch that ended another way (stopped, its driver
/// failed) is left alone.
pub fn spawn(state: Arc<AppState>, ctx: Arc<BranchCtx>, limit_ms: i64) {
    tokio::spawn(async move {
        while ctx.clock.now_ms() < limit_ms {
            if !is_live(&state, &ctx) {
                return;
            }
            tokio::time::sleep(POLL).await;
        }
        if is_live(&state, &ctx) {
            time_up(&state, &ctx).await;
        }
    });
}

fn is_live(state: &AppState, ctx: &Arc<BranchCtx>) -> bool {
    state
        .live
        .get(&ctx.branch_id)
        .is_some_and(|c| Arc::ptr_eq(&c, ctx))
}

async fn time_up(state: &AppState, ctx: &BranchCtx) {
    let minutes = ctx.rules.world_minutes.unwrap_or_default();
    let ev = json!({"part": "window", "kind": "time_up", "data": null, "clock_ms": ctx.clock.now_ms(),
                    "text": format!("the run's world time window ({minutes} min) ended")});
    let recorded = ctx.append_now(None, EventKind::WorldEvent, ev);
    if let Err(e) = recorded.await {
        eprintln!(
            "branch {}: recording the window's end: {e:#}",
            ctx.branch_id
        );
    }
    let held = ctx.window.lock().unwrap().take();
    match held.and_then(|h| h.error) {
        Some(why) => runner::finish_failed(state, ctx, why).await,
        None => runner::finish(state, ctx).await,
    };
}

/// Holds a finish asked for before the window's end: the branch's agents are ended (their
/// endpoints answer no one) and `error` is kept for the end. The clock ms the run is held until,
/// or None when the window is over (or the run has none): finish then as before.
pub fn hold(ctx: &BranchCtx, error: Option<String>) -> Option<i64> {
    let limit = limit_ms(&ctx.rules).filter(|l| ctx.clock.now_ms() < *l)?;
    for agent in ctx.roster.agents() {
        ctx.roster.revoke_token(&agent.id);
    }
    *ctx.window.lock().unwrap() = Some(Held { error });
    Some(limit)
}

/// True once a finish is held.
pub fn is_held(ctx: &BranchCtx) -> bool {
    ctx.window.lock().unwrap().is_some()
}

/// How long a driver may drive `ctx`'s run: `limit`, or its window's wall time and `grace` when
/// that is longer.
pub fn driver_limit(ctx: &BranchCtx, limit: Duration, grace: Duration) -> Duration {
    let Some(ms) = limit_ms(&ctx.rules) else {
        return limit;
    };
    let wall = Duration::from_secs_f64(ms as f64 / 1000.0 / ctx.clock.speed().max(1e-9));
    limit.max(wall + grace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_its_minutes_of_clock() {
        let rules = Rules::new(Runner::External).with_world_minutes(Some(0.05));
        assert_eq!(limit_ms(&rules), Some(3000));
        assert_eq!(limit_ms(&Rules::new(Runner::External)), None);
        assert!(valid_minutes(60.0) && valid_minutes(1440.0));
        assert!(
            !valid_minutes(0.0)
                && !valid_minutes(-1.0)
                && !valid_minutes(1441.0)
                && !valid_minutes(f64::NAN)
        );
    }
}
