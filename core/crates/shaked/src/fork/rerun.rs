//! What a rerun restores and how: per part, from its save or by replay (the restore table, with
//! the restore asked for), the fork plan, the checks that an exact restore can be exact, the
//! fingerprint check for replays, and starting the new branch's world that way.

use crate::store::Scoped;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use parts::proto::{BenchDescribe, ExactKind, FromSave, Mode};
use serde::Serialize;

use super::{
    exact::{ancestry, approximate_on_the_way, unrestored_on_the_way, world_changed},
    plan_fork_with, ForkError, ForkPlan,
};
use crate::{
    app::AppState,
    model::{Branch, Change, Event, RestoreMode, RestoredHow, RestoredPart, Team},
    recorded::{clock_at, final_saves, run_info, saves_at_except, RunInfo},
    replay::{self, ReplayError},
    restore::{restore_plan, restored, How},
    world::{self, PartStart, Plan, Saves, StartSaves, StartingPart, World, WorldError, WorldFrom},
};

/// A fork request: where, what changes, and the restore asked for.
#[derive(Debug, Clone)]
pub struct ForkRequest {
    pub at_seq: i64,
    pub changes: HashMap<String, Change>,
    pub restore: RestoreMode,
    /// Another team for the new branch, in place of `changes` (only for a run an external
    /// system drives: how its agents resume is that system's choice).
    pub team: Option<Team>,
    /// The scheduled disruptions the new branch adds or removes from its start on.
    pub disruptions: super::DisruptionChange,
}

impl ForkRequest {
    /// A fork at `at_seq` that changes nothing, restoring as `restore` asks.
    pub fn at(at_seq: i64, restore: RestoreMode) -> ForkRequest {
        ForkRequest {
            at_seq,
            changes: HashMap::new(),
            restore,
            team: None,
            disruptions: Default::default(),
        }
    }
}

/// How one part of the new world is restored.
pub struct PartRestore {
    pub part: String,
    pub how: How,
    /// The tools the part lists (its replayable inputs).
    pub tools: Vec<String>,
    /// True when the part declares replay as its exact restore.
    pub declares_replay: bool,
}

/// Everything a rerun needs before it starts but the world's plan: the run, its benchmark as it
/// is now, the fork plan, each part's restore (in world order), the step the world is restored
/// before and the clock to start at.
pub struct Rerun {
    pub run: RunInfo,
    pub bench: BenchDescribe,
    pub plan: ForkPlan,
    pub parts: Vec<PartRestore>,
    /// The world is as it was just before this seq: the fork's start, or past the parent's last
    /// step when the fork leaves the world as the parent ended.
    pub world_until: i64,
    pub clock_ms: i64,
    /// The restore asked for.
    pub mode: RestoreMode,
}

/// The parts restored by replay.
fn replayed(parts: &[PartRestore]) -> impl Iterator<Item = &PartRestore> {
    parts.iter().filter(|p| p.how == How::Replay)
}

/// The names of the parts restored by replay.
fn replayed_names(parts: &[PartRestore]) -> BTreeSet<String> {
    replayed(parts).map(|p| p.part.clone()).collect()
}

impl Rerun {
    /// Each part and how it is restored, by part name.
    pub fn hows(&self) -> BTreeMap<String, How> {
        self.parts.iter().map(|p| (p.part.clone(), p.how)).collect()
    }

    /// The new branch's `restored` record: each part and how it was restored (not at all, when
    /// the fork leaves the world as the parent ended).
    pub fn restored(&self) -> Vec<RestoredPart> {
        let mut out = restored(&self.hows());
        if self.mode == RestoreMode::NotRestored {
            out.iter_mut()
                .for_each(|r| r.how = RestoredHow::NotRestored);
        }
        out
    }

    /// Refuses a rerun that can't be what it claims: in an exact one, a part whose world changed
    /// since the run or that was restored approximately on the way; in any, a world that would
    /// come through the steps of a fork that left its world unrestored, or a replayed part whose
    /// integration's fingerprint changed (in `world`, the world's plan).
    async fn check(
        &self,
        records: Scoped<'_>,
        branch: &Branch,
        world: &Plan,
    ) -> Result<(), ForkError> {
        let replayed = replayed_names(&self.parts);
        let forks = ancestry(records, branch).await?;
        unrestored_on_the_way(&forks, self.world_until).map_err(ForkError::Refused)?;
        if self.mode == RestoreMode::Exact {
            world_changed(&self.run.world, &self.bench.world, &self.hows())
                .map_err(ForkError::Refused)?;
            approximate_on_the_way(&forks, &replayed, self.plan.start_seq)
                .map_err(ForkError::Refused)?;
        }
        check_fingerprints(&self.run, world, &replayed)
    }
}

/// Each part's restore in `mode`, in world order; the first part that can't be is refused.
fn part_restores(world: &Plan, mode: Mode) -> Result<Vec<PartRestore>, ForkError> {
    let describes = world
        .parts
        .iter()
        .map(|p| (p.part.part.as_str(), &p.describe.restore));
    let hows = restore_plan(describes, mode).map_err(ForkError::Refused)?;
    Ok(world
        .parts
        .iter()
        .map(|p| PartRestore {
            part: p.part.part.clone(),
            how: hows[&p.part.part],
            tools: p.describe.tools.iter().map(|t| t.name.clone()).collect(),
            declares_replay: p.describe.restore.exact == ExactKind::Replay,
        })
        .collect())
}

/// A part that declares replay and would start from a save it doesn't have at the start point
/// (it starts fresh in a run's first branch, so it has none before its first save) starts fresh
/// with the run's seed and is replayed up to the start point instead: exactly.
fn replay_unsaved(parts: &mut [PartRestore], restore: &Saves) {
    for p in parts.iter_mut() {
        if p.declares_replay && matches!(p.how, How::Save(_)) && !restore.contains_key(&p.part) {
            p.how = How::Replay;
        }
    }
}

/// Refuses a tools change that is empty (it would widen, not cut) or lists a tool the world
/// doesn't offer agents.
fn known_tools(world: &Plan, changes: &HashMap<String, Change>) -> Result<(), ForkError> {
    let empty = changes
        .iter()
        .find(|(_, c)| c.tools.as_ref().is_some_and(Vec::is_empty));
    if let Some((agent, _)) = empty {
        return Err(ForkError::NoTools(agent.clone()));
    }
    let listed = changes.values().flat_map(|c| c.tools.iter().flatten());
    let offered = |name: &String| world.tools.iter().any(|(t, _)| t.name == *name);
    match listed.into_iter().find(|name| !offered(name)) {
        Some(name) => Err(ForkError::UnknownTool(name.clone())),
        None => Ok(()),
    }
}

/// Refuses a fork where packages run only in sandboxes: planning its world would start the
/// world's package programs here.
fn runs_packages_here(state: &AppState) -> Result<(), ForkError> {
    let policy = state.store.layer().policy();
    let refused = crate::bind::local_only(policy, "starting a world's parts");
    refused.map_err(ForkError::SandboxOnly)
}

/// Plans a rerun of `branch` (its `events`) for `req`: the rerun, and the plan of the world it
/// starts (refused first where packages run only in sandboxes).
pub async fn prepare(
    state: &AppState,
    records: Scoped<'_>,
    branch: &Branch,
    events: &[Event],
    req: &ForkRequest,
) -> Result<(Rerun, Plan), ForkError> {
    runs_packages_here(state)?;
    if !events.iter().any(|e| e.seq == req.at_seq) {
        return Err(ForkError::UnknownSeq);
    }
    let run = run_info(events);
    let mut bench = state.hub.bench_describe(&run.benchmark).await?;
    if let Some(data) = crate::world_data::of_world(&run.world) {
        crate::world_data::apply(&mut bench, data).map_err(ForkError::Internal)?;
    }
    let world = world::plan(&state.hub, &bench).await?;
    known_tools(&world, &req.changes)?;
    let mut parts = part_restores(&world, req.restore.parts_mode())?;
    let (plan, world_until) = fork_plan(branch, events, req, &run, &parts)?;
    replay_unsaved(&mut parts, &plan.restore);
    let rerun = Rerun {
        clock_ms: clock_at(events, world_until),
        run,
        bench,
        plan,
        parts,
        world_until,
        mode: req.restore,
    };
    rerun.check(records, branch, &world).await?;
    Ok((rerun, world))
}

/// The fork plan, and the seq the world is restored before: the fork's start, or past the
/// parent's last step, from the saves it ended with, when the fork leaves the world unrestored.
fn fork_plan(
    branch: &Branch,
    events: &[Event],
    req: &ForkRequest,
    run: &RunInfo,
    parts: &[PartRestore],
) -> Result<(ForkPlan, i64), ForkError> {
    let (team, replayed) = (&branch.team, replayed_names(parts));
    if req.restore != RestoreMode::NotRestored {
        let plan = plan_fork_with(events, team, req.at_seq, &req.changes, &replayed)?;
        let until = plan.start_seq;
        return Ok((plan, until));
    }
    // No part is put back to the fork point, so none needs a save there.
    let every = parts.iter().map(|p| p.part.clone()).collect();
    let mut plan = plan_fork_with(events, team, req.at_seq, &req.changes, &every)?;
    plan.restore = end_saves(&branch.id, events, &run.base, &replayed)?;
    Ok((plan, i64::MAX))
}

/// Per part but the `replayed`, the save of the branch's state as it ended: its check's final
/// saves, else its last saves.
fn end_saves(
    branch: &str,
    events: &[Event],
    base: &Saves,
    replayed: &BTreeSet<String>,
) -> Result<Saves, ForkError> {
    if let Some(saves) = final_saves(branch, events) {
        return Ok(saves
            .into_iter()
            .filter(|(p, _)| !replayed.contains(p))
            .collect());
    }
    let last = saves_at_except(events, i64::MAX, base, replayed);
    last.map_err(|n| ForkError::NoRestorePoint { seq: n.seq })
}

/// A replayed part whose integration's fingerprint differs from the one the run recorded is
/// refused (a part the run recorded none for is not checked).
fn check_fingerprints(
    run: &RunInfo,
    world: &Plan,
    replayed: &BTreeSet<String>,
) -> Result<(), ForkError> {
    let differs = |p: &&world::PlannedPart| {
        run.fingerprints
            .get(&p.part.part)
            .is_some_and(|f| *f != p.describe.fingerprint)
    };
    match world
        .parts
        .iter()
        .filter(|p| replayed.contains(&p.part.part))
        .find(differs)
    {
        Some(p) => Err(ForkError::FingerprintMismatch {
            part: p.part.part.clone(),
        }),
        None => Ok(()),
    }
}

/// How each part of the new world starts: from the plan's save, or fresh to be replayed.
fn start_saves(rerun: &Rerun) -> Result<StartSaves, ForkError> {
    let mut out = StartSaves::new();
    for p in &rerun.parts {
        let start = match p.how {
            How::Replay => PartStart::Fresh,
            How::Save(mode) => {
                let save = rerun.plan.restore.get(&p.part).cloned().ok_or_else(|| {
                    ForkError::Internal(anyhow::anyhow!("there is no save of the part {}", p.part))
                })?;
                PartStart::Save(FromSave { save, mode })
            }
        };
        out.insert(p.part.clone(), start);
    }
    Ok(out)
}

/// A replay's failure as a fork error naming the part.
fn replay_error(part: &str, e: ReplayError) -> ForkError {
    let part = part.to_string();
    match e {
        ReplayError::Diverged { seq } => ForkError::ReplayDiverged { part, seq },
        ReplayError::Fingerprint => ForkError::FingerprintMismatch { part },
        ReplayError::Failed(e) => ForkError::Internal(e.into()),
    }
}

/// A failed world start: a part's fingerprint refusal names that part.
fn start_error(e: anyhow::Error) -> ForkError {
    let refused = e
        .chain()
        .filter_map(|c| c.downcast_ref::<WorldError>())
        .any(|w| w.code == Some(replay::FINGERPRINT_MISMATCH));
    match e.downcast_ref::<StartingPart>() {
        Some(StartingPart(part)) if refused => {
            ForkError::FingerprintMismatch { part: part.clone() }
        }
        _ => ForkError::Internal(e),
    }
}

/// Starts the new branch's world: each part from its save and moved to the fork point's clock,
/// or fresh and then replayed from the branch's recorded `events` up to the fork point, as
/// `planned`. On a failure every instance is stopped.
pub async fn start_world(
    state: &AppState,
    events: &[Event],
    new_id: &str,
    rerun: &Rerun,
    mut planned: Plan,
) -> Result<World, ForkError> {
    let from = WorldFrom::Saves(start_saves(rerun)?);
    let (hub, seed) = (state.hub.clone(), rerun.run.seed);
    let world_time = crate::recorded::world_time_of(events);
    planned.models = Some(crate::world::models::WorldModels::for_branch(
        &state.config,
        new_id,
    ));
    let world = World::start_branch(hub, &rerun.bench, planned, new_id, seed, from, world_time)
        .await
        .map_err(start_error)?;
    let ready = async {
        catch_up_saved(&world, rerun).await?;
        replay_parts(&world, events, rerun).await
    };
    if let Err(e) = ready.await {
        world.stop().await;
        return Err(e);
    }
    Ok(world)
}

/// Moves every part restored from a save to the start point's clock (a save holds the clock of
/// its step, and what the part reports on the way was recorded before the start point).
async fn catch_up_saved(world: &World, rerun: &Rerun) -> Result<(), ForkError> {
    let saved = rerun.parts.iter().filter(|p| matches!(p.how, How::Save(_)));
    for p in saved {
        let caught_up = world.advance_part(&p.part, rerun.clock_ms).await;
        caught_up.map_err(|e| ForkError::Internal(e.into()))?;
    }
    Ok(())
}

/// Replays every replayed part of a started world up to the fork point.
async fn replay_parts(world: &World, events: &[Event], rerun: &Rerun) -> Result<(), ForkError> {
    for p in replayed(&rerun.parts) {
        let inputs = replay::inputs(events, &p.part, &p.tools, rerun.world_until);
        replay::replay(world, &p.part, &inputs, rerun.clock_ms)
            .await
            .map_err(|e| replay_error(&p.part, e))?;
    }
    Ok(())
}

/// One part of a restore plan, as the rerun panel shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartPlan {
    pub part: String,
    pub how: RestoredHow,
    /// For a replay: a rough estimate of how long it takes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate_seconds: Option<f64>,
}

/// Each part of a planned rerun and how it is restored, with an estimate for replays.
pub fn part_plans(rerun: &Rerun, events: &[Event]) -> Vec<PartPlan> {
    let start = rerun.world_until;
    let unrestored = rerun.mode == RestoreMode::NotRestored;
    rerun
        .parts
        .iter()
        .map(|p| PartPlan {
            part: p.part.clone(),
            how: match unrestored {
                true => RestoredHow::NotRestored,
                false => p.how.label(),
            },
            estimate_seconds: (p.how == How::Replay)
                .then(|| replay::estimate_seconds(events, &p.part, &p.tools, start)),
        })
        .collect()
}
