//! The saves every run shares: the benchmark's base save (empty instances plus its `seed`) and
//! one gold save per task (the base plus the task's gold actions). Each is made once, under a
//! lock per hub saves folder and name, and an existing save counts as made. They are made with
//! seed 0 and name only the parts that keep saves (a part with only replay starts fresh instead).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use anyhow::{bail, Context, Result};
use parts::{
    proto::{Action, BenchDescribe, GoldActions, GoldParams, SeedParams, SetupCall},
    Hub,
};
use serde_json::Value;

use crate::{restore::in_shared_saves, save_calls::save_exists};

use super::{
    from_shared,
    ids::{base_names, gold_names, temp_branch_id},
    plan::{plan, Plan},
    request, Saves, StartSaves, World,
};

/// The locks of [`name_lock`], one per saves folder and name.
type NameLocks = HashMap<(PathBuf, String), Arc<tokio::sync::Mutex<()>>>;

/// One lock per name in the saves folder `saves` (a hub's): a save is made once even when two
/// runs start together, and hubs on other folders never wait for each other.
async fn name_lock(saves: &Path, name: &str) -> tokio::sync::OwnedMutexGuard<()> {
    static LOCKS: LazyLock<Mutex<NameLocks>> = LazyLock::new(Default::default);
    let key = (saves.to_path_buf(), name.to_string());
    let l = LOCKS.lock().unwrap().entry(key).or_default().clone();
    l.lock_owned().await
}

/// The seed of the base and gold worlds (shared by every run).
pub const SHARED_SEED: u64 = 0;

/// True when every save in `names` exists.
async fn saves_exist(hub: &Hub, planned: &Plan, names: &Saves) -> Result<bool> {
    for p in &planned.parts {
        if let Some(save) = names.get(&p.part.part) {
            if !save_exists(hub, &p.part.integration, save).await? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// `names` of the parts that keep saves.
fn kept_names(planned: &Plan, mut names: Saves) -> Saves {
    names.retain(|part, _| {
        let p = planned.parts.iter().find(|p| &p.part.part == part);
        p.is_some_and(|p| in_shared_saves(&p.describe.restore))
    });
    names
}

/// Makes `names` (of the parts that keep saves) unless they exist: a short-lived world (empty
/// when `from` is None, else from those shared saves), `build` on it, then every part saved
/// under its name.
async fn ensure_saves(
    hub: &Arc<Hub>,
    bench: &BenchDescribe,
    names: Saves,
    from: Option<Saves>,
    build: impl AsyncFnOnce(&World) -> Result<()>,
) -> Result<Saves> {
    let planned = plan(hub, bench).await?;
    let names = kept_names(&planned, names);
    let joined = names.values().cloned().collect::<Vec<_>>().join(",");
    let _g = name_lock(hub.saves_root(), &joined).await;
    if saves_exist(hub, &planned, &names).await? {
        return Ok(names);
    }
    let from = from.map(|f| from_shared(&f, &planned.parts));
    build_saves(hub, bench, planned, from, &names, build).await?;
    Ok(names)
}

/// A short-lived world started from `from` (empty when None), `build` on it, then its parts
/// saved under `names`; the world is stopped either way.
async fn build_saves(
    hub: &Arc<Hub>,
    bench: &BenchDescribe,
    planned: Plan,
    from: Option<StartSaves>,
    names: &Saves,
    build: impl AsyncFnOnce(&World) -> Result<()>,
) -> Result<()> {
    let branch = temp_branch_id();
    let start = World::start_parts(
        hub.clone(),
        bench,
        planned,
        &branch,
        SHARED_SEED,
        from.as_ref(),
    );
    let world = start.await?;
    let built = async {
        build(&world).await?;
        world.save_all_as(names).await.map_err(anyhow::Error::from)
    }
    .await;
    world.stop().await;
    built
}

/// The base save of each part, made once (empty instances, the benchmark's `seed`, save).
pub async fn ensure_base(hub: &Arc<Hub>, bench: &BenchDescribe) -> Result<Saves> {
    let seed = async |w: &World| -> Result<()> {
        let params = SeedParams {
            connections: w.connections.clone(),
        };
        let _: Value = request(w.bench_client()?, &w.bench_label(), &params).await?;
        Ok(())
    };
    ensure_saves(hub, bench, base_names(bench), None, seed)
        .await
        .with_context(|| format!("making the base save of {}", bench.name))
}

/// True when the base save of every part that keeps saves exists (nothing is made).
pub async fn has_base(hub: &Arc<Hub>, bench: &BenchDescribe) -> Result<bool> {
    let planned = plan(hub, bench).await?;
    let names = kept_names(&planned, base_names(bench));
    saves_exist(hub, &planned, &names).await
}

/// The gold save of each part for `task`: the base plus the task's setup and its gold actions,
/// made once.
pub async fn ensure_gold(hub: &Arc<Hub>, bench: &BenchDescribe, task: &str) -> Result<Saves> {
    let names = gold_names(bench, task);
    let from = ensure_base(hub, bench).await?;
    let of_task = bench.tasks.iter().find(|t| t.id == task);
    let setup = of_task.map_or(&[][..], |t| &t.setup[..]);
    ensure_saves(hub, bench, names, Some(from), async |w: &World| {
        set_up(w, setup).await?;
        apply_gold(w, task).await
    })
    .await
    .with_context(|| format!("making the gold save of task {task}"))
}

/// Makes a task's setup calls, as a run's start does; the first that fails is the error.
async fn set_up(world: &World, calls: &[SetupCall]) -> Result<()> {
    for c in calls {
        let out = world.call_part_tool(&c.part, &c.tool, c.args.clone(), 0);
        if let Some(e) = out.await?.error {
            bail!("the task's setup call {} failed: {e}", c.tool);
        }
    }
    Ok(())
}

/// Runs the task's gold actions. At least one gold write must succeed (a failing gold action
/// changes nothing, as upstream).
async fn apply_gold(world: &World, task: &str) -> Result<()> {
    let params = GoldParams {
        task: task.to_string(),
    };
    let gold: GoldActions = request(world.bench_client()?, &world.bench_label(), &params).await?;
    let mut wrote = false;
    for Action { name, args } in gold.actions {
        let c = world.call_tool(&name, args, 0).await?;
        wrote |= c.output.error.is_none() && !c.output.writes.is_empty();
    }
    if !wrote {
        bail!("task {task}: no gold write succeeded");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{path::Path, time::Duration};

    use super::name_lock;

    /// True when `name_lock(saves, name)` is had within a moment.
    async fn had_at_once(saves: &str, name: &str) -> bool {
        let soon = Duration::from_millis(200);
        tokio::time::timeout(soon, name_lock(Path::new(saves), name))
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn two_hubs_making_saves_of_one_name_do_not_wait_for_each_other() {
        let _held = name_lock(Path::new("/saves/hub-a"), "base-retail").await;
        assert!(had_at_once("/saves/hub-b", "base-retail").await);
    }

    #[tokio::test]
    async fn one_hub_makes_saves_of_one_name_one_at_a_time() {
        let held = name_lock(Path::new("/saves/hub-c"), "base-retail").await;
        assert!(!had_at_once("/saves/hub-c", "base-retail").await);
        drop(held);
        assert!(had_at_once("/saves/hub-c", "base-retail").await);
    }
}
