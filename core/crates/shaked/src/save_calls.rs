//! Calls on saves outside a running world, by integration: read keys, compare, drop, stop a
//! leftover instance, measure and clean up. A part's own error stays a `parts::CallError` inside
//! the returned error, so callers can downcast it for its code.

use std::{collections::BTreeSet, future::Future, time::Duration};

use anyhow::{Context, Result};
use parts::{
    proto::{
        kept, AdvanceParams, DiffParams, Differing, Docs, GcParams, GcResult, InspectParams,
        SaveId, StartParams, Target, Usage, WorldPart,
    },
    request::{GetUsage, Stop},
    CallError, Hub, Request,
};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::{model::Event, recorded::run_info};

async fn request<T: DeserializeOwned, R: Request>(
    hub: &Hub,
    integration: &str,
    r: &R,
) -> Result<T> {
    let client = hub.client(integration).await?;
    in_context(client.request(r).await, integration, R::METHOD)
}

/// `request` without the client's deadline: waits until the integration answers or exits.
async fn request_until_done<T: DeserializeOwned, R: Request>(
    hub: &Hub,
    integration: &str,
    r: &R,
) -> Result<T> {
    let client = hub.client(integration).await?;
    in_context(client.request_until_done(r).await, integration, R::METHOD)
}

/// `save`, as `inspect` and `diff` name it.
fn saved(save: &str) -> Target {
    Target::Save {
        save: save.to_string(),
    }
}

/// A call's result, its error saying which integration and method failed.
fn in_context<T>(r: Result<T, CallError>, integration: &str, method: &str) -> Result<T> {
    r.map_err(anyhow::Error::from)
        .with_context(|| format!("the {integration} integration: {method}"))
}

/// The part's own error, `(code, message)`, when `e` is one.
pub fn part_refusal(e: &anyhow::Error) -> Option<(i64, &str)> {
    match e.downcast_ref::<CallError>() {
        Some(CallError::Part { code, message, .. }) => Some((*code, message.as_str())),
        _ => None,
    }
}

/// The part's own error code, when `e` is one.
pub fn part_code(e: &anyhow::Error) -> Option<i64> {
    part_refusal(e).map(|(code, _)| code)
}

/// The docs of `keys` in `save` (empty for no keys: `inspect` with none would read everything).
pub async fn inspect(
    hub: &Hub,
    integration: &str,
    save: &str,
    keys: &[String],
) -> Result<Map<String, Value>> {
    if keys.is_empty() {
        return Ok(Map::new());
    }
    let params = InspectParams {
        target: saved(save),
        keys: keys.to_vec(),
    };
    let d: Docs = request(hub, integration, &params).await?;
    Ok(d.docs)
}

/// The docs of `keys` in `target`, every doc for no keys.
async fn inspect_target(
    hub: &Hub,
    integration: &str,
    target: Target,
    keys: &[String],
) -> Result<Map<String, Value>> {
    let params = InspectParams {
        target,
        keys: keys.to_vec(),
    };
    let d: Docs = request(hub, integration, &params).await?;
    Ok(d.docs)
}

/// The docs of `keys` in `save`, every doc for no keys.
pub async fn inspect_all(
    hub: &Hub,
    integration: &str,
    save: &str,
    keys: &[String],
) -> Result<Map<String, Value>> {
    inspect_target(hub, integration, saved(save), keys).await
}

/// The docs of `keys` (every doc for none) of a clock part as it stands once moved on from a save
/// to `clock_ms`: `start` (from the save, in a scratch instance) started, advanced, inspected and
/// stopped, also when a step fails. The events the advance reports are dropped: they were recorded.
pub async fn inspect_advanced(
    hub: &Hub,
    integration: &str,
    start: &StartParams,
    clock_ms: i64,
    keys: &[String],
) -> Result<Map<String, Value>> {
    let instance = start.instance.clone();
    let docs = async {
        let _: Value = request(hub, integration, start).await?;
        let advance = AdvanceParams {
            instance: instance.clone(),
            clock_ms,
        };
        let _: Value = request(hub, integration, &advance).await?;
        let target = Target::Instance {
            instance: instance.clone(),
        };
        inspect_target(hub, integration, target, keys).await
    }
    .await;
    let stopped: Result<Value> = request(hub, integration, &Stop::instance(&instance)).await;
    if let Err(e) = stopped {
        eprintln!("stopping the scratch instance {instance}: {e:#}");
    }
    docs
}

/// The keys that differ between two saves of one integration.
pub async fn diff(hub: &Hub, integration: &str, a: &str, b: &str) -> Result<Vec<String>> {
    let params = DiffParams {
        a: saved(a),
        b: saved(b),
    };
    let d: Differing = request(hub, integration, &params).await?;
    Ok(d.differing)
}

/// True when the integration has `save` (probed with `inspect` of a key no part has).
pub async fn save_exists(hub: &Hub, integration: &str, save: &str) -> Result<bool> {
    let params = InspectParams {
        target: saved(save),
        keys: vec!["shakedown/probe".into()],
    };
    match request::<Value, _>(hub, integration, &params).await {
        Ok(_) => Ok(true),
        Err(e) if part_code(&e) == Some(-32002) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Drops `save` and the folder shaked made for it; false when the integration answers that it
/// has no such save (-32002: already gone, which counts as dropped).
pub async fn drop_save(hub: &Hub, integration: &str, save: &str) -> Result<bool> {
    let params = SaveId {
        save: save.to_string(),
    };
    let existed = match request::<Value, _>(hub, integration, &params).await {
        Ok(_) => true,
        Err(e) if part_code(&e) == Some(-32002) => false,
        Err(e) => return Err(e),
    };
    hub.remove_save_dir(integration, save);
    Ok(existed)
}

/// Stops `instance` (a no-op for an instance that is not running).
pub async fn stop(hub: &Hub, integration: &str, instance: &str) -> Result<()> {
    let _: Value = request(hub, integration, &Stop::instance(instance)).await?;
    Ok(())
}

/// Bytes the integration keeps.
pub async fn usage(hub: &Hub, integration: &str) -> Result<u64> {
    let u: Usage = request(hub, integration, &GetUsage {}).await?;
    Ok(u.bytes)
}

/// Drops the integration's saves not in `keep` (an entry ending in `*` is a prefix) and its
/// instances not running, then the save folders `keep` doesn't keep; None when the integration
/// has no `gc`. Waits as long as the integration's gc takes: it may stop instances, so it must
/// be over before anything starts new ones.
pub async fn gc(hub: &Hub, integration: &str, keep: &[String]) -> Result<Option<GcResult>> {
    let params = GcParams {
        keep: keep.to_vec(),
    };
    let r = match request_until_done(hub, integration, &params).await {
        Ok(r) => r,
        Err(e) if part_code(&e) == Some(-32601) => return Ok(None),
        Err(e) => return Err(e),
    };
    for save in hub.save_dirs(integration) {
        if !kept(keep, &save) {
            hub.remove_save_dir(integration, &save);
        }
    }
    Ok(Some(r))
}

/// The parts of a branch's world with their integrations: as its `RunStart` recorded them; for
/// a run recorded before integrations, as the run's benchmark names them now; none for a branch
/// without `RunStart`.
pub async fn world_of(hub: &Hub, events: &[Event]) -> Result<Vec<WorldPart>> {
    let info = run_info(events);
    if !info.legacy {
        return Ok(info.world);
    }
    Ok(hub.bench_describe(&info.benchmark).await?.world)
}

/// The part called `part` in `world`.
pub fn part_named<'a>(world: &'a [WorldPart], part: &str) -> Option<&'a WorldPart> {
    world.iter().find(|p| p.part == part)
}

/// The integration of `part` in `world`.
pub fn integration_of<'a>(world: &'a [WorldPart], part: &str) -> Option<&'a str> {
    part_named(world, part).map(|p| p.integration.as_str())
}

/// The integrations `benchmark`'s world uses, each once.
pub async fn integrations(hub: &Hub, benchmark: &str) -> Result<BTreeSet<String>> {
    let bench = hub.bench_describe(benchmark).await?;
    Ok(bench.world.into_iter().map(|p| p.integration).collect())
}

/// `f`'s result, or the plain error text: its own, or that it did not `verb` (answer, finish)
/// within `limit`.
pub async fn bounded<T>(
    limit: Duration,
    verb: &str,
    f: impl Future<Output = Result<T>>,
) -> Result<T, String> {
    match tokio::time::timeout(limit, f).await {
        Ok(r) => r.map_err(|e| format!("{e:#}")),
        Err(_) => Err(format!("did not {verb} within {} s", limit.as_secs())),
    }
}
