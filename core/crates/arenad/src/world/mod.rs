//! A branch's world: one instance of each part of the benchmark's world, run by the part's
//! integration program (shared by every branch through the hub), plus the benchmark program for
//! the benchmark's own tools.

mod clocked;
mod gates;
mod hosts;
mod ids;
mod links;
mod plan;
mod saving;
mod scoring;
mod standard;
mod systems;
mod view;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail, Result};
use parts::{
    proto::{
        BenchCallTool, BenchDescribe, BenchToolOutput, CallToolParams, Describe, Effect, ExactKind,
        FromSave, Gate, HttpRoute, HttpRoutes, Link, Mode, StartParams, Target, ToolOutput,
        ToolSchema, ViewInfo, WorldPart,
    },
    request::{Connect, GetHttpRoutes, Stop},
    CallError, Client, Hub, Request,
};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::restore::{in_shared_saves, keeps_saves};

pub use clocked::PartEvent;
pub use hosts::{HostRoute, Hosts, HTTP_TOOL};
pub use ids::{instance_id, new_save_id};
pub use links::link_args;
pub use plan::{plan, Plan, PlannedPart, Route};
pub use saving::StepSaves;
pub use standard::{ensure_base, ensure_gold, SHARED_SEED};

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
    /// Milliseconds spent checking the world's gates before the call (arena's own work).
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
        let output = ToolOutput {
            error: Some(message),
            ..Default::default()
        };
        Called::new(output, None)
    }
}

struct Part {
    name: String,
    integration: String,
    instance: String,
    client: Arc<Client>,
    describe: Describe,
}

impl Part {
    fn label(&self) -> String {
        format!("the {} integration", self.name)
    }

    /// The part's live instance, as `inspect` and `diff` name it.
    fn target(&self) -> Target {
        Target::Instance {
            instance: self.instance.clone(),
        }
    }

    /// True when the part keeps saves (a part with only replay is never saved).
    fn keeps_saves(&self) -> bool {
        keeps_saves(&self.describe.restore)
    }

    /// True when the part is in the shared base and gold saves (so it can be checked).
    fn in_shared_saves(&self) -> bool {
        in_shared_saves(&self.describe.restore)
    }

    /// `call_tool` on the part's instance.
    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        clock_ms: i64,
    ) -> Result<ToolOutput, WorldError> {
        let params = CallToolParams {
            instance: self.instance.clone(),
            name: name.to_string(),
            args,
            clock_ms,
        };
        request(&self.client, &self.label(), &params).await
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

/// What starting one part came to: the part when it started (with its save and connection info),
/// and the error when starting or connecting it failed.
struct StartedPart {
    part: Option<Part>,
    save: Option<String>,
    connection: Option<Value>,
    /// The hosts the part's instance serves through the proxy.
    routes: Vec<HttpRoute>,
    error: Option<anyhow::Error>,
}

impl StartedPart {
    fn failed(part: Option<Part>, save: Option<String>, e: anyhow::Error) -> StartedPart {
        StartedPart {
            part,
            save,
            connection: None,
            routes: vec![],
            error: Some(e),
        }
    }
}

/// Starts `p`'s instance for `branch_id` (from its save in `from`, if any) and connects it when a
/// benchmark may reach it.
async fn start_part(
    hub: &Hub,
    p: PlannedPart,
    branch_id: &str,
    seed: u64,
    from: Option<&StartSaves>,
) -> StartedPart {
    let (part, save) = match begin_part(hub, p, branch_id, seed, from).await {
        Ok(begun) => begun,
        Err(e) => return StartedPart::failed(None, None, e),
    };
    let connected = match connect(&part).await {
        Ok(connection) => connection,
        Err(e) => return StartedPart::failed(Some(part), save, e),
    };
    match http_routes(&part).await {
        Ok(routes) => StartedPart {
            part: Some(part),
            save,
            connection: connected,
            routes,
            error: None,
        },
        Err(e) => StartedPart::failed(Some(part), save, e),
    }
}

/// The part's instance started: the part, and the save it started from.
async fn begin_part(
    hub: &Hub,
    p: PlannedPart,
    branch_id: &str,
    seed: u64,
    from: Option<&StartSaves>,
) -> Result<(Part, Option<String>)> {
    let instance = instance_id(branch_id, &p.part.part);
    let (params, save) = start_params(&p, &instance, seed, from)?;
    let part = Part {
        client: hub.client(&p.part.integration).await?,
        name: p.part.part,
        integration: p.part.integration,
        instance,
        describe: p.describe,
    };
    let started: Result<Value, WorldError> = request(&part.client, &part.label(), &params).await;
    started.map_err(|e| anyhow::Error::from(e).context(StartingPart(part.name.clone())))?;
    Ok((part, save))
}

pub struct World {
    hub: Arc<Hub>,
    bench: String,
    bench_client: Option<Arc<Client>>,
    parts: Vec<Part>,
    tools: Vec<(ToolSchema, Route)>,
    links: Vec<Link>,
    gates: Vec<Gate>,
    /// Present when the benchmark draws the world's view.
    view: Option<ViewInfo>,
    connections: Map<String, Value>,
    /// The hosts its parts serve through the proxy.
    hosts: Hosts,
    track: Mutex<Track>,
}

fn stopped_message(what: &str, tail: &str) -> String {
    match tail.trim() {
        "" => format!("{what} stopped"),
        tail => format!("{what} stopped: {tail}"),
    }
}

/// A request to a program; a stopped program is a fatal `WorldError` naming `what`.
async fn request<T: DeserializeOwned, R: Request>(
    client: &Client,
    what: &str,
    r: &R,
) -> Result<T, WorldError> {
    let method = R::METHOD;
    client.request(r).await.map_err(|e| match e {
        CallError::Stopped(tail) => WorldError {
            message: stopped_message(what, &tail),
            fatal: true,
            code: None,
        },
        CallError::Part { code, .. } => WorldError {
            code: Some(code),
            ..WorldError::other(format!("{what}: {method} failed: {e}"))
        },
        other => WorldError::other(format!("{what}: {method} failed: {other}")),
    })
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

    fn new(hub: Arc<Hub>, bench: &str, bench_client: Option<Arc<Client>>, plan: Plan) -> World {
        World {
            hub,
            bench: bench.to_string(),
            bench_client,
            parts: vec![],
            tools: plan.tools,
            links: plan.links,
            gates: plan.gates,
            view: plan.view,
            connections: Map::new(),
            hosts: Hosts::default(),
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
        let starts = (parts.into_iter()).map(|p| start_part(&hub, p, branch_id, seed, from));
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
            if let Some(p) = &s.part {
                served.push((p.name.clone(), p.describe.restore, routes));
            }
            if let Some(e) = self.take_started(s) {
                failed.get_or_insert(e);
            }
        }
        match Hosts::build(served) {
            Ok(hosts) => self.hosts = hosts,
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

    /// The hosts the world's parts serve through the proxy.
    pub fn hosts(&self) -> &Hosts {
        &self.hosts
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

    /// True when some part uses the arena clock.
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

    /// Runs `part`'s own tool `name` (for a replay).
    pub async fn call_part_tool(
        &self,
        part: &str,
        name: &str,
        args: Value,
        clock_ms: i64,
    ) -> Result<ToolOutput, WorldError> {
        self.part(part)?.call_tool(name, args, clock_ms).await
    }

    async fn call_bench_tool(&self, name: &str, args: Value) -> Result<Called, WorldError> {
        let params = BenchCallTool {
            name: name.to_string(),
            args,
            connections: self.connections.clone(),
        };
        let o: BenchToolOutput =
            request(self.bench_client()?, &self.bench_label(), &params).await?;
        if let Some(error) = self.bench_part_error(&o) {
            return Ok(Called::error(error));
        }
        let output = ToolOutput {
            result: o.result,
            error: o.error,
            reads: o.reads,
            writes: o.writes,
            ..Default::default()
        };
        Ok(Called::new(output, Some(o.part)))
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

/// How `p` starts given `from` (None: every part empty). A part that is not in the shared saves
/// and isn't named starts fresh; any other part must be named.
fn part_start<'a>(p: &PlannedPart, from: Option<&'a StartSaves>) -> Result<Option<&'a FromSave>> {
    let Some(from) = from else {
        return Ok(None);
    };
    let name = &p.part.part;
    match from.get(name) {
        Some(PartStart::Save(f)) => Ok(Some(f)),
        Some(PartStart::Fresh) => Ok(None),
        None if !in_shared_saves(&p.describe.restore) => Ok(None),
        None => Err(anyhow!("there is no save to start the part {name} from")),
    }
}

/// `start` params of a part, and the save it starts from (None for an empty start).
fn start_params(
    p: &PlannedPart,
    instance: &str,
    seed: u64,
    from: Option<&StartSaves>,
) -> Result<(StartParams, Option<String>)> {
    let part: &WorldPart = &p.part;
    let mut params = StartParams {
        instance: instance.to_string(),
        config: part.config.clone(),
        seed,
        clock_ms: 0,
        from: None,
        world_time: None,
    };
    let Some(f) = part_start(p, from)? else {
        return Ok((params, None));
    };
    params.from = Some(f.clone());
    Ok((params, Some(f.save.clone())))
}

/// `connect` info of a part a benchmark may reach (exact saves only); None when it has none.
async fn connect(part: &Part) -> Result<Option<Value>> {
    if part.describe.restore.exact != ExactKind::Save {
        return Ok(None);
    }
    match part
        .client
        .request::<Value>(&Connect::instance(&part.instance))
        .await
    {
        Ok(info) => Ok(Some(info)),
        Err(CallError::Part { code: -32601, .. }) => Ok(None),
        Err(CallError::Stopped(tail)) => bail!(stopped_message(&part.label(), &tail)),
        Err(e) => bail!("connecting to the part {}: {e}", part.name),
    }
}

/// The hosts a part's instance serves through the proxy; none when it doesn't answer
/// `http_routes`.
async fn http_routes(part: &Part) -> Result<Vec<HttpRoute>> {
    let asked = GetHttpRoutes::instance(&part.instance);
    match part.client.request::<HttpRoutes>(&asked).await {
        Ok(served) => Ok(served.routes),
        Err(CallError::Part { code: -32601, .. }) => Ok(vec![]),
        Err(CallError::Stopped(tail)) => bail!(stopped_message(&part.label(), &tail)),
        Err(e) => bail!("asking the part {} which hosts it serves: {e}", part.name),
    }
}

/// A running instance and the integration that runs it.
struct Instance {
    integration: String,
    id: String,
}

async fn stop_instances(hub: &Hub, instances: Vec<Instance>) {
    for Instance {
        integration,
        id: instance,
    } in instances
    {
        let r = async {
            hub.client(&integration)
                .await?
                .request::<Value>(&Stop::instance(&instance))
                .await
                .map_err(anyhow::Error::from)
        };
        if let Err(e) = r.await {
            eprintln!("world: could not stop {instance} ({integration}): {e:#}");
        }
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
