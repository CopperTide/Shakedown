//! A branch's world: one instance of each part of the benchmark's world, run by the part's
//! integration program (shared by every branch through the hub), plus the benchmark program for
//! the benchmark's own tools.

mod clocked;
mod gates;
mod hosts;
mod ids;
mod links;
mod part;
mod plan;
mod saving;
mod scoring;
mod standard;
mod systems;
mod view;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, RwLock},
};

use anyhow::{anyhow, Result};
use parts::{
    proto::{
        BenchCallTool, BenchDescribe, BenchToolOutput, Effect, ExactKind, FromSave, Gate, Link,
        Mode, ToolOutput, ToolSchema, ViewInfo, WorldTimeParams,
    },
    Client, Hub,
};
use serde_json::{Map, Value};

use part::{http_routes, request, start_part, stop_instances, Instance, Part, StartedPart};

pub use clocked::PartEvent;
pub use hosts::{HostRoute, Hosts, HTTP_TOOL};
pub use ids::{instance_id, new_save_id};
pub use links::link_args;
pub use plan::{plan, Plan, PlannedPart, Route};
pub use saving::StepSaves;
pub use standard::{ensure_base, ensure_gold, has_base, SHARED_SEED};

/// `{part: save}`.
pub type Saves = BTreeMap<String, String>;

/// How a part's instance starts.
#[derive(Debug, Clone, PartialEq)]
pub enum PartStart {
    /// From a save, in a mode.
    Save(FromSave),
    /// Empty, with the run's seed at clock 0 (to be replayed, or a part that keeps no saves).
    Fresh,
}

/// Per part: how it starts. A part that keeps no saves and is not named starts fresh.
pub type StartSaves = BTreeMap<String, PartStart>;

/// The closest a part restores from a save: exactly when it keeps exact saves, else
/// approximately.
fn closest_mode(p: &PlannedPart) -> Mode {
    match p.describe.restore.exact {
        ExactKind::Save => Mode::Exact,
        _ => Mode::Approximate,
    }
}

/// Each part of `parts` named in `saves` started from its save, as closely as it restores (a
/// shared base or gold save).
fn from_shared(saves: &Saves, parts: &[PlannedPart]) -> StartSaves {
    parts
        .iter()
        .filter_map(|p| {
            let save = saves.get(&p.part.part)?.clone();
            let mode = closest_mode(p);
            Some((
                p.part.part.clone(),
                PartStart::Save(FromSave { save, mode }),
            ))
        })
        .collect()
}

/// Where a world starts from.
pub enum WorldFrom {
    /// The benchmark's base save (made on first use), each part as closely as it restores.
    Base,
    /// Per part: the save and how to restore it.
    Saves(StartSaves),
}

/// A failed world call. `fatal` when a program has stopped: the branch can't go on. `code` is
/// the part's own error code when it refused the call.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct WorldError {
    pub message: String,
    pub fatal: bool,
    pub code: Option<i64>,
}

impl WorldError {
    fn other(message: String) -> WorldError {
        WorldError {
            message,
            fatal: false,
            code: None,
        }
    }
}

/// The context of a part's failed `start`: which part it was.
#[derive(Debug, Clone, thiserror::Error)]
#[error("starting the part {0}")]
pub struct StartingPart(pub String);

/// What a tool call gave back, and which part it ran on (None for an unknown tool).
#[derive(Debug, Clone)]
pub struct Called {
    pub output: ToolOutput,
    pub part: Option<String>,
    /// Milliseconds spent checking the world's gates before the call (Shakedown's own work).
    pub gate_ms: f64,
}

impl Called {
    fn new(output: ToolOutput, part: Option<String>) -> Called {
        Called {
            output,
            part,
            gate_ms: 0.0,
        }
    }

    /// A tool error that reached no part.
    fn error(message: String) -> Called {
        Called::new(failed(message), None)
    }
}

/// The output of a call that failed with `message`.
fn failed(message: String) -> ToolOutput {
    ToolOutput {
        error: Some(message),
        ..Default::default()
    }
}

#[derive(Default)]
struct Track {
    /// The save that holds each part's current state.
    last: Saves,
    /// Parts written since their last save (a save failed).
    dirty: BTreeSet<String>,
    /// Clock parts whose `advance` reported a change since their last save.
    changed: BTreeSet<String>,
    stopped: bool,
}

pub struct World {
    hub: Arc<Hub>,
    bench: String,
    bench_client: Option<Arc<Client>>,
    parts: Vec<Part>,
    tools: Vec<(ToolSchema, Route)>,
    /// The benchmark's tools a setup calls on a part (see [`World::call_part_tool`]).
    setup_bench_tools: BTreeSet<String>,
    links: Vec<Link>,
    gates: Vec<Gate>,
    /// Present when the benchmark draws the world's view.
    view: Option<ViewInfo>,
    connections: Map<String, Value>,
    /// The hosts its parts serve through the proxy (asked again after a routed part's save).
    hosts: RwLock<Hosts>,
    track: Mutex<Track>,
}

impl World {
    /// Starts `bench`'s world for `branch_id`: one instance per part, from the base save or from
    /// the given saves, all with `seed` at clock 0 (passed with a save too). Refuses a world with
    /// a tool-name clash.
    pub async fn start(
        hub: Arc<Hub>,
        bench: &BenchDescribe,
        branch_id: &str,
        seed: u64,
        from: WorldFrom,
    ) -> Result<World> {
        let planned = plan(&hub, bench).await?;
        World::start_planned(hub, bench, planned, branch_id, seed, from).await
    }

    /// `start` with the world's `plan` already made.
    pub async fn start_planned(
        hub: Arc<Hub>,
        bench: &BenchDescribe,
        planned: Plan,
        branch_id: &str,
        seed: u64,
        from: WorldFrom,
    ) -> Result<World> {
        let from = match from {
            WorldFrom::Base => from_shared(&ensure_base(&hub, bench).await?, &planned.parts),
            WorldFrom::Saves(m) => m,
        };
        World::start_parts(hub, bench, planned, branch_id, seed, Some(&from)).await
    }

    /// `start_planned` for a branch: its parts' world time has clock 0 at `epoch_ms` and runs at
    /// the speed branch clocks run at.
    pub async fn start_branch(
        hub: Arc<Hub>,
        bench: &BenchDescribe,
        mut planned: Plan,
        branch_id: &str,
        seed: u64,
        from: WorldFrom,
        epoch_ms: i64,
    ) -> Result<World> {
        let speed = crate::clock::speed_from_env();
        planned.world_time = Some(WorldTimeParams { epoch_ms, speed });
        World::start_planned(hub, bench, planned, branch_id, seed, from).await
    }

    fn new(hub: Arc<Hub>, bench: &str, bench_client: Option<Arc<Client>>, plan: Plan) -> World {
        World {
            hub,
            bench: bench.to_string(),
            bench_client,
            parts: vec![],
            tools: plan.tools,
            setup_bench_tools: plan.setup_bench_tools,
            links: plan.links,
            gates: plan.gates,
            view: plan.view,
            connections: Map::new(),
            hosts: RwLock::default(),
            track: Mutex::default(),
        }
    }

    /// Starts every part (from `from`, or empty when None) at once and connects the ones that
    /// can be: a world restores in the time of its slowest part. On a failure the parts that
    /// started are stopped.
    async fn start_parts(
        hub: Arc<Hub>,
        bench: &BenchDescribe,
        planned: Plan,
        branch_id: &str,
        seed: u64,
        from: Option<&StartSaves>,
    ) -> Result<World> {
        let mut planned = planned;
        let parts = std::mem::take(&mut planned.parts);
        let bench_client = hub.client(&bench.name).await?;
        let wt = planned.world_time;
        let starts = (parts.into_iter()).map(|p| start_part(&hub, p, branch_id, seed, from, wt));
        let started = futures_util::future::join_all(starts).await;
        let mut world = World::new(hub, &bench.name, Some(bench_client), planned);
        if let Some(e) = world.take_all_started(started) {
            world.stop().await;
            return Err(e);
        }
        Ok(world)
    }

    /// Adds the parts that started (see `take_started`) and the hosts they serve; the first
    /// error, or why their hosts can't be served together.
    fn take_all_started(&mut self, started: Vec<StartedPart>) -> Option<anyhow::Error> {
        let mut failed = None;
        let mut served = vec![];
        for mut s in started {
            let routes = std::mem::take(&mut s.routes);
            if let Some(p) = &mut s.part {
                p.serves_http = !routes.is_empty();
                served.push((p.name.clone(), p.describe.restore, routes));
            }
            if let Some(e) = self.take_started(s) {
                failed.get_or_insert(e);
            }
        }
        match Hosts::build(served) {
            Ok(hosts) => *self.hosts.get_mut().unwrap() = hosts,
            Err(e) => drop(failed.get_or_insert(anyhow!(e))),
        }
        failed
    }

    /// Adds a part that started (in world order) with its save and connection; the error of one
    /// that failed. A part that started and then failed to connect is added too, so it is stopped
    /// with the world.
    fn take_started(&mut self, s: StartedPart) -> Option<anyhow::Error> {
        if let Some(part) = s.part {
            let track = self.track.get_mut().unwrap();
            if let Some(save) = s.save {
                track.last.insert(part.name.clone(), save);
            }
            if let Some(info) = s.connection {
                self.connections.insert(part.name.clone(), info);
            }
            self.parts.push(part);
        }
        s.error
    }

    pub fn bench(&self) -> &str {
        &self.bench
    }

    /// The hosts the world's parts serve through the proxy (held for reading: never across an
    /// await).
    pub fn hosts(&self) -> std::sync::RwLockReadGuard<'_, Hosts> {
        self.hosts.read().unwrap()
    }

    /// Asks `p` again which hosts it serves and serves those; a failure is logged and the old
    /// routes stay.
    async fn refresh_routes(&self, p: &Part) {
        let replaced = match http_routes(p).await {
            Ok(routes) => self.hosts.write().unwrap().replace(&p.name, routes),
            Err(e) => Err(format!("{e:#}")),
        };
        if let Err(e) = replaced {
            eprintln!("world: the routes of {} after its save: {e}", p.name);
        }
    }

    fn bench_label(&self) -> String {
        format!("the {} benchmark", self.bench)
    }

    fn bench_client(&self) -> Result<&Client, WorldError> {
        self.bench_client.as_deref().ok_or_else(|| WorldError {
            fatal: true,
            ..WorldError::other("no benchmark program".into())
        })
    }

    /// The tools agents get (MCP shape plus `effect`).
    pub fn tools(&self) -> impl Iterator<Item = &ToolSchema> {
        self.tools.iter().map(|(t, _)| t)
    }

    /// True when `name` is one of the world's tools agents get.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools().any(|t| t.name == name)
    }

    pub fn is_write(&self, name: &str) -> bool {
        self.tools()
            .any(|t| t.name == name && t.effect == Effect::Write)
    }

    /// The instance id of `part`.
    pub fn instance(&self, part: &str) -> Option<&str> {
        self.part(part).ok().map(|p| p.instance.as_str())
    }

    /// The part names, in world order.
    pub fn part_names(&self) -> Vec<String> {
        self.parts.iter().map(|p| p.name.clone()).collect()
    }

    /// True when some part uses the world clock.
    pub fn uses_clock(&self) -> bool {
        self.parts.iter().any(|p| p.describe.uses_clock)
    }

    /// The parts that can't be checked against the task's gold saves (they are not in them).
    pub fn unchecked_parts(&self) -> Vec<String> {
        self.parts
            .iter()
            .filter(|p| !p.in_shared_saves())
            .map(|p| p.name.clone())
            .collect()
    }

    /// Each part's integration `fingerprint`.
    pub fn fingerprints(&self) -> Map<String, Value> {
        self.parts
            .iter()
            .map(|p| (p.name.clone(), p.describe.fingerprint.clone()))
            .collect()
    }

    /// The save that holds each part's current state (the start saves until a write is saved).
    pub fn saves(&self) -> Saves {
        self.track.lock().unwrap().last.clone()
    }

    fn part(&self, name: &str) -> Result<&Part, WorldError> {
        self.parts
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| WorldError::other(format!("there is no part {name} in this world")))
    }

    /// Runs a tool: the benchmark's own, or the part's that lists it. An unknown tool, or one a
    /// gate holds back, is a tool error and reaches no part.
    pub async fn call_tool(
        &self,
        name: &str,
        args: Value,
        clock_ms: i64,
    ) -> Result<Called, WorldError> {
        let route = self.tools.iter().find(|(t, _)| t.name == name);
        let Some(route) = route.map(|(_, r)| *r) else {
            return Ok(Called::error(format!("unknown tool {name}")));
        };
        let since = std::time::Instant::now();
        let refused = self.gate_refusal(name, &args).await?;
        let gate_ms = crate::overhead::ms_since(since);
        let mut called = match (refused, route) {
            (Some(output), _) => Called::new(output, None),
            (None, Route::Bench) => self.call_bench_tool(name, args).await?,
            (None, Route::Part(i)) => {
                let p = &self.parts[i];
                let output = p.call_tool(name, args, clock_ms).await?;
                Called::new(output, Some(p.name.clone()))
            }
        };
        called.gate_ms = gate_ms;
        Ok(called)
    }

    /// Runs `part`'s own tool `name` (for a link, a setup or a replay), or the benchmark's tool
    /// `name` that a setup calls on `part` (it must answer for `part`).
    pub async fn call_part_tool(
        &self,
        part: &str,
        name: &str,
        args: Value,
        clock_ms: i64,
    ) -> Result<ToolOutput, WorldError> {
        if !self.setup_bench_tools.contains(name) {
            return self.part(part)?.call_tool(name, args, clock_ms).await;
        }
        let (output, answered) = self.bench_output(name, args).await?;
        match answered {
            Some(p) if p != part => Ok(failed(format!(
                "the benchmark's {name} acted on {p}, not on {part}"
            ))),
            _ => Ok(output),
        }
    }

    async fn call_bench_tool(&self, name: &str, args: Value) -> Result<Called, WorldError> {
        let (output, part) = self.bench_output(name, args).await?;
        Ok(Called::new(output, part))
    }

    /// The benchmark's answer to its tool `name` and the part it acted on (none when the answer
    /// can't stand: then the output is the error).
    async fn bench_output(
        &self,
        name: &str,
        args: Value,
    ) -> Result<(ToolOutput, Option<String>), WorldError> {
        let params = BenchCallTool {
            name: name.to_string(),
            args,
            connections: self.connections.clone(),
        };
        let o: BenchToolOutput =
            request(self.bench_client()?, &self.bench_label(), &params).await?;
        if let Some(error) = self.bench_part_error(&o) {
            return Ok((failed(error), None));
        }
        let output = ToolOutput {
            result: o.result,
            error: o.error,
            reads: o.reads,
            writes: o.writes,
            events: o.events,
        };
        Ok((output, Some(o.part)))
    }

    /// Why a benchmark tool's answer can't stand: it names an unknown part, or wrote to a part
    /// without exact saves (whose replay can't repeat a benchmark's direct writes).
    fn bench_part_error(&self, o: &BenchToolOutput) -> Option<String> {
        let Ok(p) = self.part(&o.part) else {
            return Some(format!("the benchmark named an unknown part {}", o.part));
        };
        let exact = p.describe.restore.exact == ExactKind::Save;
        (!exact && !o.writes.is_empty()).then(|| {
            format!(
                "benchmark tools write to {} directly, so it must have exact saves",
                o.part
            )
        })
    }

    /// Every part's instance, and marks the world stopped; empty when it already was.
    fn take_instances(&self) -> Vec<Instance> {
        let mut t = self.track.lock().unwrap();
        if std::mem::replace(&mut t.stopped, true) {
            return vec![];
        }
        self.parts
            .iter()
            .map(|p| Instance {
                integration: p.integration.clone(),
                id: p.instance.clone(),
            })
            .collect()
    }

    /// Stops every instance (once). A program that has stopped is started again for this, so a
    /// crashed part's instance is still cleaned up. Failures are logged.
    pub async fn stop(&self) {
        stop_instances(&self.hub, self.take_instances()).await;
    }
}

impl Drop for World {
    fn drop(&mut self) {
        // Best effort: an instance must not outlive its world.
        let todo = self.take_instances();
        if todo.is_empty() {
            return;
        }
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let hub = self.hub.clone();
            rt.spawn(async move { stop_instances(&hub, todo).await });
        }
    }
}
