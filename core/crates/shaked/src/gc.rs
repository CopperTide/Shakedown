//! Disk hygiene: the bytes the shakedown keeps, deleting a run's saves, and the startup cleanup of
//! saves no event names.

use crate::store::Scoped;
use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use parts::{
    proto::{kept, GcResult, WorldPart},
    Hub,
};
use serde::Serialize;

use crate::{
    model::Event,
    recorded::{named_saves, NamedSave},
    save_calls::{self, integration_of, integrations, world_of},
    store::Store,
    world::instance_id,
};

/// Bytes the shakedown keeps: the `usage` of each integration `benchmark`'s world uses, plus the
/// save folders. An integration that is not installed keeps nothing and is skipped; so does a
/// benchmark that is not (a hosted shaked has none: its packages run in sandboxes).
pub async fn usage(hub: &Hub, benchmark: &str) -> Result<u64> {
    let mut bytes = hub.saves_bytes();
    if hub.registry().get(benchmark).is_none() {
        return Ok(bytes);
    }
    let world = integrations(hub, benchmark).await?;
    for i in world.iter().filter(|i| hub.registry().get(i).is_some()) {
        bytes += save_calls::usage(hub, i).await?;
    }
    Ok(bytes)
}

/// The shared saves of every benchmark and version, as `gc` keep prefixes.
const SHARED_KEEP: [&str; 2] = ["base_*", "gold_*"];

/// The shared saves every run starts from or is checked against: never dropped with a run.
fn is_shared(save: &str) -> bool {
    kept(&SHARED_KEEP, save)
}

/// Per integration, what deleting a run frees: the saves to drop and the instances to stop.
#[derive(Default)]
struct Doomed {
    saves: BTreeMap<String, BTreeSet<String>>,
    instances: BTreeMap<String, BTreeSet<String>>,
}

impl Doomed {
    /// Adds one branch: the saves its events name (but the shared ones) and its instances.
    fn add_branch(&mut self, branch: &str, events: &[Event], world: &[WorldPart]) {
        for NamedSave { part, save } in named_saves(branch, events) {
            if is_shared(&save) {
                continue;
            }
            match integration_of(world, &part) {
                Some(i) => {
                    self.saves.entry(i.to_string()).or_default().insert(save);
                }
                None => {
                    eprintln!("delete: no integration for part {part} of {branch}; {save} kept")
                }
            }
        }
        for p in world {
            let instance = instance_id(branch, &p.part);
            self.instances
                .entry(p.integration.clone())
                .or_default()
                .insert(instance);
        }
    }
}

/// Deletes the run from the store, then drops every save its events name (except the shared
/// base and gold saves) and stops its instances. The number of saves dropped.
pub async fn delete_run(store: Scoped<'_>, hub: &Hub, run_id: &str) -> Result<usize> {
    let mut doomed = Doomed::default();
    for b in store.run_branches(run_id).await? {
        let events = store.events(&b).await?;
        let world = world_of(hub, &events).await?;
        doomed.add_branch(&b, &events, &world);
    }
    store.delete_run(run_id).await?;
    Ok(drop_doomed(hub, &doomed, run_id).await)
}

/// Drops the saves and stops the instances; failures are logged. The number of saves dropped
/// (a save that was already gone doesn't count).
async fn drop_doomed(hub: &Hub, doomed: &Doomed, run_id: &str) -> usize {
    let mut dropped = 0;
    for (integration, list) in &doomed.saves {
        for save in list {
            match save_calls::drop_save(hub, integration, save).await {
                Ok(existed) => dropped += usize::from(existed),
                Err(e) => eprintln!("delete run {run_id}: could not drop {save}: {e:#}"),
            }
        }
    }
    for (integration, list) in &doomed.instances {
        for instance in list {
            if let Err(e) = save_calls::stop(hub, integration, instance).await {
                eprintln!("delete run {run_id}: could not stop {instance}: {e:#}");
            }
        }
    }
    dropped
}

/// Every save the store's events name, plus the base and gold saves of every benchmark.
async fn keep_list(store: &Store) -> Result<Vec<String>> {
    let mut by_branch: BTreeMap<String, Vec<Event>> = BTreeMap::new();
    for e in store.events_naming_saves().await? {
        by_branch.entry(e.branch_id.clone()).or_default().push(e);
    }
    let mut keep: BTreeSet<String> = by_branch
        .iter()
        .flat_map(|(b, events)| named_saves(b, events))
        .map(|n| n.save)
        .collect();
    keep.extend(SHARED_KEEP.map(String::from));
    Ok(keep.into_iter().collect())
}

/// What a cleanup dropped: the integrations' totals, and the save folders that are gone.
#[derive(Debug, Default)]
pub struct GcReport {
    pub result: GcResult,
    /// Ids of the saves whose folders were removed, each once. An integration may drop a save
    /// that has no folder, so this can be shorter than `result.dropped`.
    pub removed: Vec<String>,
    /// The integrations whose gc failed, with the plain error.
    pub errors: Vec<GcError>,
}

#[derive(Debug, Serialize)]
pub struct GcError {
    pub integration: String,
    pub message: String,
}

/// Asks each integration `benchmark`'s world uses to drop the saves no event names (and its
/// instances), and removes the save folders not kept. Integrations without `gc` are skipped; a
/// failing one is logged. No deadline: each integration's gc runs to its end (or until the
/// integration exits), so a caller holding the cleanup lock keeps it while gc can still stop
/// instances.
pub async fn run_gc(store: &Store, hub: &Hub, benchmark: &str) -> Result<GcReport> {
    let keep = keep_list(store).await?;
    let mut report = GcReport::default();
    let mut removed = BTreeSet::new();
    for i in integrations(hub, benchmark).await? {
        let before = hub.save_dirs(&i);
        match save_calls::gc(hub, &i, &keep).await {
            Ok(Some(r)) => {
                report.result.dropped += r.dropped;
                report.result.bytes += r.bytes;
            }
            Ok(None) => println!("gc: the {i} integration has no gc; skipped"),
            Err(e) => {
                eprintln!("gc: {e:#}");
                report.errors.push(GcError {
                    integration: i.clone(),
                    message: format!("{e:#}"),
                });
            }
        }
        let after = hub.save_dirs(&i);
        removed.extend(before.into_iter().filter(|s| !after.contains(s)));
    }
    report.removed = removed.into_iter().collect();
    Ok(report)
}

/// `run_gc` at startup (none of the world's instances runs yet). Logs
/// `gc: dropped N saves (X MB)`.
pub async fn run_startup_gc(store: &Store, hub: &Hub, benchmark: &str) -> Result<GcResult> {
    let total = run_gc(store, hub, benchmark).await?.result;
    println!(
        "gc: dropped {} saves ({} MB)",
        total.dropped,
        total.bytes / 1_000_000
    );
    Ok(total)
}

#[cfg(test)]
mod usage_tests {
    use super::*;

    /// A shaked with no benchmark installed (hosted: packages run in sandboxes) counts its save
    /// folders alone, instead of failing `/api/health`.
    #[tokio::test]
    async fn usage_without_a_benchmark_is_the_saves_alone() {
        let root =
            std::env::temp_dir().join(format!("shaked-gc-{}", uuid::Uuid::new_v4().simple()));
        let (packages, saves) = (root.join("packages"), root.join("saves"));
        std::fs::create_dir_all(&packages).unwrap();
        std::fs::create_dir_all(saves.join("s1")).unwrap();
        std::fs::write(saves.join("s1/data"), [0u8; 10]).unwrap();
        let registry = parts::Registry::scan(&[packages]).unwrap();
        let hub = Hub::new(registry, root.join("bin"), saves);
        let bytes = usage(&hub, "").await;
        let _ = std::fs::remove_dir_all(&root);
        assert!(bytes.unwrap() >= 10);
    }
}
