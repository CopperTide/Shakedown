//! Moving a stepped world on while its team waits: to the earliest of the next thing a part has
//! scheduled (`wake_at_ms`), the lead's next look (its epoch since its last wake) and the run's
//! horizon. After the lead finishes early, the world runs on, with no agent, to the horizon, so
//! costs that land later are still scored.

use parts::proto::SteppedClock;

use super::board::WakePoint;
use crate::exec::{BranchCtx, ToolExec};

/// The most moves a finished run makes on its way to the horizon; past them it goes straight
/// there (a part that keeps asking for the next ms can't hold the run up).
const MAX_MOVES: usize = 10_000;

/// Where a quiet stepped world moves on to from `now`: the earliest of `wake` (a part's), `look_at`
/// (the lead's look) and `horizon`, never before `now` and never past the horizon; at a tie the
/// horizon wins, then the look. None when there is none of them.
pub fn next_point(
    now: i64,
    wake: Option<i64>,
    look_at: Option<i64>,
    horizon: Option<i64>,
) -> Option<(i64, WakePoint)> {
    let points = [
        horizon.map(|h| (h, WakePoint::Horizon)),
        look_at.map(|l| (l, WakePoint::Look)),
        wake.map(|w| (w, WakePoint::Part)),
    ];
    let (at, to) = points.into_iter().flatten().min_by_key(|(at, _)| *at)?;
    let at = at.max(now);
    match horizon {
        Some(h) if at >= h => Some((h.max(now), WakePoint::Horizon)),
        _ => Some((at, to)),
    }
}

/// Runs a finished branch's world on to the horizon, through every part's wake, with no agent
/// (none when the clock has no horizon). Each move is recorded.
pub async fn run_to_horizon(
    exec: &ToolExec,
    ctx: &BranchCtx,
    clock: &SteppedClock,
) -> Result<(), String> {
    let Some(horizon) = clock.horizon_ms else {
        return Ok(());
    };
    exec.catch_up(ctx).await?;
    for _ in 0..MAX_MOVES {
        let (world, now) = (&ctx.world, ctx.clock.now_ms());
        if now >= horizon {
            return Ok(());
        }
        let wake = world.due_by(now).map(|_| now).or(world.next_wake(now));
        let (to_ms, to) =
            next_point(now, wake, None, Some(horizon)).unwrap_or((horizon, WakePoint::Horizon));
        exec.step_world(ctx, to_ms, to.word()).await?;
    }
    exec.step_world(ctx, horizon, WakePoint::Horizon.word())
        .await
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    #[test]
    fn the_earliest_point_wins_and_the_horizon_caps_it() {
        let h = Some(600 * MIN);
        assert_eq!(
            next_point(0, Some(90 * MIN), Some(60 * MIN), h),
            Some((60 * MIN, WakePoint::Look))
        );
        assert_eq!(
            next_point(0, Some(30 * MIN), Some(60 * MIN), h),
            Some((30 * MIN, WakePoint::Part))
        );
        assert_eq!(
            next_point(0, Some(900 * MIN), None, h),
            Some((600 * MIN, WakePoint::Horizon))
        );
        assert_eq!(
            next_point(0, None, None, h),
            Some((600 * MIN, WakePoint::Horizon))
        );
        assert_eq!(
            next_point(0, None, None, None),
            None,
            "nowhere to go: stuck"
        );
        assert_eq!(
            next_point(0, Some(MIN), None, None),
            Some((MIN, WakePoint::Part))
        );
    }

    #[test]
    fn a_point_already_passed_is_now_and_a_tie_goes_to_the_horizon() {
        let now = 120 * MIN;
        assert_eq!(
            next_point(now, None, Some(60 * MIN), None),
            Some((now, WakePoint::Look))
        );
        assert_eq!(
            next_point(now, Some(now), None, None),
            Some((now, WakePoint::Part))
        );
        let h = Some(180 * MIN);
        assert_eq!(
            next_point(now, Some(180 * MIN), Some(180 * MIN), h),
            Some((180 * MIN, WakePoint::Horizon))
        );
        assert_eq!(
            next_point(200 * MIN, None, None, h),
            Some((200 * MIN, WakePoint::Horizon))
        );
    }
}
