//! Scoring a branch: its world frozen and checked against the task's expected end state.

use std::time::Instant;

use anyhow::Result;

use super::lifecycle::Ending;
use crate::{app::AppState, exec::BranchCtx, model::*, overhead::Cat, world};

/// Once the agents are done the world stands still: no tick and no clock while it is scored. The
/// clock parts are first moved on to the frozen clock (a wall clock's last tick came up to a tick
/// before it), and the move recorded, so they are scored at the end. Freezing twice is freezing
/// once.
pub(super) async fn freeze(state: &AppState, ctx: &BranchCtx) {
    ctx.stop_ticker().await;
    ctx.clock.freeze();
    let exec = crate::exec::ToolExec::new(state.store.clone());
    if let Err(e) = exec.settle(ctx).await {
        eprintln!("branch {}: moving the world to its end: {e}", ctx.branch_id);
    }
}

/// The world scored as it stands: frozen, then checked.
pub(crate) async fn score_now(state: &AppState, ctx: &BranchCtx) -> Result<Ending> {
    freeze(state, ctx).await;
    check(state, ctx).await
}

/// Scores the world and records the Check. A branch on which an agent broke a rule fails with it,
/// whatever the checks say.
async fn check(state: &AppState, ctx: &BranchCtx) -> Result<Ending> {
    let since = Instant::now();
    let check = score(state, ctx).await?;
    ctx.overhead.add_since(Cat::Score, since);
    let ending = match ctx.lifecycle.violation() {
        Some(why) => Ending::failed(why),
        None => verdict(&check, &ctx.world.part_names()),
    };
    let payload = serde_json::to_value(&check)?;
    ctx.append(None, EventKind::Check, payload).await?;
    Ok(ending)
}

pub const NOTHING_CHECKED: &str =
    "no part of this world can be checked against the task's expected result";

/// How a scored branch ends: passed when some part was checked and every checked part matches;
/// failed with `NOTHING_CHECKED` when no part could be checked.
fn verdict(check: &CheckPayload, parts: &[String]) -> Ending {
    if check.unchecked.len() >= parts.len() {
        return Ending::failed(NOTHING_CHECKED);
    }
    match check.passed {
        true => Ending::new(BranchStatus::Passed, None),
        false => Ending::new(BranchStatus::Failed, None),
    }
}

/// Saves every part's final state and has the benchmark check the world's end state.
async fn score_by_checks(ctx: &BranchCtx) -> Result<CheckPayload> {
    // Behind the gate: no late tool call lands between the final save and the checks.
    let _gate = ctx.gate().await;
    let final_saves = ctx.world.save_all().await?;
    let checks = ctx.world.score(&ctx.task_id).await?;
    let failed = checks.iter().filter(|c| !c.passed);
    let failed: Vec<String> = failed.map(|c| c.name.clone()).collect();
    Ok(CheckPayload {
        passed: !checks.is_empty() && failed.is_empty(),
        gold_db: None,
        differing_rows: Some(failed),
        checks: Some(checks),
        final_saves: Some(final_saves),
        unchecked: vec![],
    })
}

/// Saves every part's final state and compares the world with the task's gold saves; parts not
/// in them are listed as unchecked. A benchmark that scores itself has no gold saves.
async fn score(state: &AppState, ctx: &BranchCtx) -> Result<CheckPayload> {
    let bench = state.hub.bench_describe(ctx.world.bench()).await?;
    if bench.scored {
        return score_by_checks(ctx).await;
    }
    let gold = world::ensure_gold(&state.hub, &bench, &ctx.task_id).await?;
    // Behind the gate: no late tool call lands between the final save and the diff.
    let _gate = ctx.gate().await;
    let final_saves = ctx.world.save_all().await?;
    let differing = ctx.world.diff_with(&gold).await?;
    let unchecked = ctx.world.unchecked_parts();
    let gold_db = ctx
        .world
        .part_names()
        .iter()
        .find_map(|p| gold.get(p))
        .cloned();
    Ok(CheckPayload {
        passed: differing.is_empty() && unchecked.len() < ctx.world.part_names().len(),
        gold_db,
        differing_rows: Some(differing),
        checks: None,
        final_saves: Some(final_saves),
        unchecked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(passed: bool, unchecked: &[&str]) -> CheckPayload {
        CheckPayload {
            passed,
            gold_db: None,
            differing_rows: Some(vec![]),
            checks: None,
            final_saves: None,
            unchecked: unchecked.iter().map(|p| p.to_string()).collect(),
        }
    }

    #[test]
    fn a_branch_passes_only_when_some_part_was_checked() {
        let parts = ["db".to_string(), "sim".to_string()];
        assert_eq!(
            verdict(&check(true, &["sim"]), &parts),
            Ending::new(BranchStatus::Passed, None)
        );
        assert_eq!(
            verdict(&check(false, &[]), &parts),
            Ending::new(BranchStatus::Failed, None)
        );
        let none = verdict(&check(false, &["db", "sim"]), &parts);
        assert_eq!(
            none,
            Ending::new(BranchStatus::Failed, Some(NOTHING_CHECKED.to_string()))
        );
    }
}
