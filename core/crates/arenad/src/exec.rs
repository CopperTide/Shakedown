mod dispatch;
mod http;
mod lifecycle;
mod links;
mod mailbox;
mod roster;
mod save_point;
mod setup;
mod talk;
mod view;

use std::sync::Arc;

use parts::proto::{Effect, ToolOutput, WorldEvent};
use serde_json::{json, Value};

use crate::{
    clock::BranchClock,
    flow::{FlowControl, STOPPED},
    model::*,
    overhead::{Cat, Ledger},
    recorded::LEGACY_PART,
    store::Store,
    ticker::Ticker,
    world::{Called, PartEvent, Saves, World, WorldError},
};

pub use dispatch::{Said, CLOSED, DELEGATE, FINISH, REPORT};
pub use http::{HttpAnswer, HttpCall, NOT_RUNNING};
pub use lifecycle::Lifecycle;
pub use links::LINK_ACTOR;
pub use mailbox::Mailbox;
pub use roster::Roster;
pub use setup::SETUP_ACTOR;
pub use talk::{handoff_schema, mail_for, said_text, HANDOFF, INBOX, MESSAGE};

/// One running branch: its world, team, arena clock and per-agent handoff notes.
pub struct BranchCtx {
    pub branch_id: String,
    pub world: Arc<World>,
    pub task_id: String,
    /// The branch's arena clock (from 0 at the run's start, from the fork point on a fork).
    pub clock: BranchClock,
    /// The rules the branch runs under (from its run's `RunStart`).
    pub rules: Rules,
    /// Its spend limit and running estimate (`rules.max_cost_usd`).
    pub budget: crate::budget::Budget,
    /// Held across every tool call so world order, saves and event order agree.
    gate: tokio::sync::Mutex<()>,
    /// Paused, stopped or running: calls wait while the branch is paused.
    pub flow: FlowControl,
    /// Its team, the agents that joined it and their tokens.
    pub roster: Roster,
    /// Its agents' handoff notes and what they say.
    pub mailbox: Mailbox,
    /// Its ticker and how far it is through its life.
    pub lifecycle: Lifecycle,
    /// The arena's time on this branch since its last event (see [`crate::overhead`]).
    pub overhead: Ledger,
}

impl BranchCtx {
    /// A branch whose clock starts at 0 (at the speed `ARENA_CLOCK_SPEED` sets), under the rules
    /// of runs recorded before runners (see [`Rules::legacy`]) until `with_rules` says otherwise.
    pub fn new(branch_id: String, world: Arc<World>, team: Team, task_id: String) -> BranchCtx {
        BranchCtx {
            branch_id,
            world,
            task_id,
            clock: BranchClock::from_env(0),
            rules: Rules::legacy(),
            budget: crate::budget::Budget::default(),
            gate: tokio::sync::Mutex::new(()),
            flow: FlowControl::default(),
            roster: Roster::new(team),
            mailbox: Mailbox::default(),
            lifecycle: Lifecycle::default(),
            overhead: Ledger::default(),
        }
    }

    /// The same branch under `rules`.
    pub fn with_rules(mut self, rules: Rules) -> BranchCtx {
        self.budget = crate::budget::Budget::new(rules.max_cost_usd);
        self.rules = rules;
        self
    }

    /// The same branch with `clock` as its arena clock.
    pub fn with_clock(mut self, clock: BranchClock) -> BranchCtx {
        self.clock = clock;
        self
    }

    /// Starts the clock ticker (when the world has clock parts and none runs yet).
    pub async fn start_ticker(self: &Arc<Self>, store: &Arc<Store>) {
        let mut t = self.lifecycle.ticker.lock().await;
        if t.is_none() {
            *t = Ticker::start(ToolExec::new(store.clone()), self.clone());
        }
    }

    /// Stops the clock ticker and waits for it to end.
    pub async fn stop_ticker(&self) {
        if let Some(t) = self.lifecycle.ticker.lock().await.take() {
            t.stop().await;
        }
    }

    /// Holds the branch gate: no tool call runs on the world until the guard drops.
    pub async fn gate(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.gate.lock().await
    }

    /// Records `payload` as an event of its kind on this branch (see `append`).
    pub async fn record<P: EventPayload>(
        &self,
        store: &Store,
        agent: Option<&str>,
        payload: &P,
    ) -> anyhow::Result<Event> {
        let payload = serde_json::to_value(payload)?;
        self.append(store, agent, P::KIND, payload).await
    }

    /// Records an event of this branch with the world clock paused: recording is arena's work.
    pub async fn append(
        &self,
        store: &Store,
        agent: Option<&str>,
        kind: EventKind,
        payload: Value,
    ) -> anyhow::Result<Event> {
        let _paused = self.clock.paused();
        self.append_now(store, agent, kind, payload).await
    }

    /// Records an event of this branch, the world clock running on (for what happens outside the
    /// agents' and the world's work, like a driver's exit). It carries the overhead built up since
    /// the branch's last event; the time it takes is overhead of the kind `record`.
    pub async fn append_now(
        &self,
        store: &Store,
        agent: Option<&str>,
        kind: EventKind,
        payload: Value,
    ) -> anyhow::Result<Event> {
        let since = std::time::Instant::now();
        let (id, built_up) = (&self.branch_id, self.overhead.take());
        let ev = store.append_with(id, agent, kind, payload, built_up).await;
        self.overhead.add_since(Cat::Record, since);
        ev
    }

    /// Pauses the branch: new tool calls and model calls wait and the world clock stands still
    /// until `resume`. Records `Pause`; false when the branch wasn't running.
    pub async fn pause(&self, store: &Store) -> anyhow::Result<bool> {
        if !self.flow.pause() {
            return Ok(false);
        }
        self.clock.pause();
        self.record(store, None, &PausePayload {}).await?;
        Ok(true)
    }

    /// Resumes a paused branch: the calls that wait go on and the clock runs again. Records
    /// `Resume` with how long the pause lasted (counted as arena's time, not the run's); false
    /// when the branch wasn't paused.
    pub async fn resume(&self, store: &Store) -> anyhow::Result<bool> {
        let Some(lasted) = self.flow.resume() else {
            return Ok(false);
        };
        let ms = lasted.as_secs_f64() * 1000.0;
        self.overhead.add(Cat::Pause, ms);
        self.clock.resume();
        let resumed = ResumePayload {
            paused_ms: ms.round() as i64,
        };
        self.record(store, None, &resumed).await?;
        Ok(true)
    }

    /// Waits for the branch gate, counting the wait as arena's overhead.
    async fn gate_timed(&self) -> tokio::sync::MutexGuard<'_, ()> {
        let since = std::time::Instant::now();
        let guard = self.gate.lock().await;
        self.overhead.add_since(Cat::Wait, since);
        guard
    }
}

/// The single place where tool calls are executed, recorded and saved.
pub struct ToolExec {
    pub store: Arc<Store>,
}

/// The tools `agent` gets on `branch`, in MCP shape (`name`, `description`, `inputSchema`): the
/// world's (only its read tools when the agent's team made it read-only, only those its setup lists
/// when it lists some), and the tools agents talk with (see `talk`).
pub fn tool_schemas(branch: &BranchCtx, agent: &str) -> Vec<Value> {
    let read_only = branch.roster.read_only(agent);
    let mut list: Vec<Value> = branch
        .world
        .tools()
        .filter(|t| !read_only || t.effect != Effect::Write)
        .filter(|t| branch.roster.may_use(agent, &t.name))
        .map(|t| json!({"name": t.name, "description": t.description, "inputSchema": t.input_schema}))
        .collect();
    list.extend(talk::schemas(branch, agent));
    list
}

/// What running a tool call left to record besides its outcome.
#[derive(Default)]
struct CallFacts {
    part: Option<String>,
    reads: Vec<String>,
    /// The world's keys the call wrote.
    writes: Vec<String>,
    /// The part's internal keys it wrote: the part is saved for them, but the step is no write.
    internal: Vec<String>,
    saves: Option<Saves>,
    save_error: Option<String>,
    clock_ms: Option<i64>,
    paused_ms: Option<i64>,
    duration_ms: Option<f64>,
    /// Events the call gave the world, reported after the call is recorded.
    events: Vec<PartEvent>,
}

impl CallFacts {
    /// What a call on `part` gave back besides its outcome: its reads, writes (the world's and
    /// the internal ones apart) and events.
    fn of_output(part: Option<String>, output: ToolOutput, clock_ms: i64) -> CallFacts {
        let (writes, internal) = ToolOutput::split_writes(output.writes);
        CallFacts {
            events: part_events(part.as_deref(), output.events, clock_ms),
            part,
            reads: output.reads,
            writes,
            internal,
            clock_ms: Some(clock_ms),
            ..CallFacts::default()
        }
    }

    /// True when the call changed its part at all, internal keys included: it must be saved.
    fn changed_part(&self) -> bool {
        !self.writes.is_empty() || !self.internal.is_empty()
    }

    /// True when the call wrote the world's state (internal keys aside): a step that wrote.
    fn wrote_world(&self) -> bool {
        self.part.is_some() && !self.writes.is_empty()
    }
}

fn tool_call_payload(
    name: &str,
    args: Value,
    outcome: &Result<Value, String>,
    tool_call_id: Option<String>,
    f: CallFacts,
) -> ToolCallPayload {
    ToolCallPayload {
        name: name.to_string(),
        args,
        result: outcome.as_ref().ok().cloned().unwrap_or(Value::Null),
        error: outcome.as_ref().err().cloned(),
        reads: json!(f.reads),
        writes: json!(f.writes),
        internal: f.internal,
        snapshot: f.saves.as_ref().and_then(|s| s.get(LEGACY_PART).cloned()),
        snapshot_error: f.save_error,
        tool_call_id,
        part: f.part,
        saves: f.saves,
        clock_ms: f.clock_ms,
        paused_ms: f.paused_ms,
        duration_ms: f.duration_ms,
    }
}

/// A tool's outcome as an agent sees it: its result, or its error.
pub fn outcome_of(output: &ToolOutput) -> Result<Value, String> {
    match &output.error {
        Some(e) => Err(e.clone()),
        None => Ok(output.result.clone().unwrap_or(Value::Null)),
    }
}

fn world_event_payload(e: PartEvent) -> WorldEventPayload {
    WorldEventPayload {
        part: e.part,
        kind: e.event.kind,
        text: e.event.text,
        data: e.event.data,
        clock_ms: e.clock_ms,
    }
}

/// `events` a tool call on `part` gave at `clock_ms`, as part events.
fn part_events(part: Option<&str>, events: Box<[WorldEvent]>, clock_ms: i64) -> Vec<PartEvent> {
    let Some(part) = part else {
        return vec![];
    };
    events
        .into_vec()
        .into_iter()
        .map(|event| PartEvent {
            part: part.to_string(),
            event,
            clock_ms,
        })
        .collect()
}

/// Why `agent` may not run `name` now: the branch can't go on, its setup doesn't give it the tool,
/// or it writes and the agent's team made it read-only.
fn refusal(branch: &BranchCtx, agent: &str, name: &str) -> Option<String> {
    if let Some(f) = branch.lifecycle.fatal() {
        return Some(f);
    }
    if branch.world.has_tool(name)
        && branch.roster.has_agent(agent)
        && !branch.roster.may_use(agent, name)
    {
        return Some(format!(
            "{name} is not available to this agent: its setup does not list it"
        ));
    }
    (branch.world.is_write(name) && branch.roster.read_only(agent)).then(|| {
        "Write tools are not available to this agent: its team made it read-only".to_string()
    })
}

/// Saves what changed with a step (the part written to, the clock parts that changed) with the
/// clock paused, and records the saves, a failure and the pause.
async fn save_step(branch: &BranchCtx, written: Option<&str>, facts: &mut CallFacts) {
    let paused = branch.clock.paused();
    let step = branch.world.save_changed(written).await;
    if !step.saves.is_empty() || step.error.is_some() {
        facts.paused_ms = Some(paused.elapsed_ms());
        branch.overhead.add(Cat::Save, paused.elapsed_ms_f64());
    }
    facts.saves = (!step.saves.is_empty()).then_some(step.saves);
    facts.save_error = step.error.map(|e| failure(branch, e));
}

/// The message of a failed world call; a stopped program also ends the branch.
fn failure(branch: &BranchCtx, e: WorldError) -> String {
    if e.fatal {
        branch.lifecycle.set_fatal(&e.message);
        e.message
    } else {
        format!("internal error: {e}")
    }
}

impl ToolExec {
    pub fn new(store: Arc<Store>) -> ToolExec {
        ToolExec { store }
    }

    /// The JSON given back to the agent: the result, or `{"error": msg}`.
    pub async fn run(
        &self,
        branch: &BranchCtx,
        agent: &str,
        name: &str,
        args: Value,
        tool_call_id: Option<String>,
    ) -> Value {
        match self
            .run_outcome(branch, agent, name, args, tool_call_id)
            .await
        {
            Ok(v) => v,
            Err(e) => json!({"error": e}),
        }
    }

    /// Advances the world's clock parts to the branch clock behind the gate (a clock tick).
    pub async fn tick(&self, branch: &BranchCtx) -> Result<(), String> {
        let _gate = branch.gate().await;
        if branch.lifecycle.fatal().is_some() {
            return Ok(());
        }
        self.advance(branch, branch.clock.now_ms()).await
    }

    /// Advances the clock parts to `clock_ms` and reports the events they give.
    async fn advance(&self, branch: &BranchCtx, clock_ms: i64) -> Result<(), String> {
        let events = branch
            .world
            .advance_all(clock_ms)
            .await
            .map_err(|e| failure(branch, e))?;
        self.report(branch, events).await.map(drop)
    }

    /// Runs a tool call on the branch's world at the branch clock (the clock parts advanced to
    /// it first), then saves what changed.
    async fn execute(
        &self,
        branch: &BranchCtx,
        agent: &str,
        name: &str,
        args: &Value,
    ) -> (Result<Value, String>, CallFacts) {
        let mut facts = CallFacts::default();
        if let Some(e) = refusal(branch, agent, name) {
            return (Err(e), facts);
        }
        let clock = branch.clock.now_ms();
        facts.clock_ms = Some(clock);
        if let Err(e) = self.advance(branch, clock).await {
            return (Err(e), facts);
        }
        let since = std::time::Instant::now();
        let called = match branch.world.call_tool(name, args.clone(), clock).await {
            Ok(c) => c,
            Err(e) => return (Err(failure(branch, e)), facts),
        };
        let gate_ms = called.gate_ms;
        facts.duration_ms = Some((since.elapsed().as_secs_f64() * 1000.0 - gate_ms).max(0.0));
        branch.overhead.add(Cat::Gate, gate_ms);
        let outcome = outcome_of(&called.output);
        let Called { output, part, .. } = called;
        facts = CallFacts {
            duration_ms: facts.duration_ms,
            ..CallFacts::of_output(part, output, clock)
        };
        // A refused call may have changed its part anyway (a disruption fired), and a call that
        // changed only internal keys must keep them: saved too.
        let written = facts.part.clone().filter(|_| facts.changed_part());
        save_step(branch, written.as_deref(), &mut facts).await;
        (outcome, facts)
    }

    /// Like `run`, but keeps success and tool error apart.
    pub async fn run_outcome(
        &self,
        branch: &BranchCtx,
        agent: &str,
        name: &str,
        args: Value,
        tool_call_id: Option<String>,
    ) -> Result<Value, String> {
        branch.flow.go().await.map_err(|_| STOPPED.to_string())?;
        let _gate = branch.gate_timed().await;
        if branch.lifecycle.is_closed() {
            return Err(CLOSED.to_string());
        }
        if talk::is_talk(branch, name) {
            return self.talk(branch, agent, name, args, tool_call_id).await;
        }
        let (outcome, mut facts) = self.execute(branch, agent, name, &args).await;
        let events = std::mem::take(&mut facts.events);
        let wrote = facts.wrote_world();
        let payload = tool_call_payload(name, args, &outcome, tool_call_id, facts);
        let seq = self
            .record_seq(branch, Some(agent), EventKind::ToolCall, &payload)
            .await?;
        let linked = self.report(branch, events).await?;
        if let Some(seq_of) = linked.or(wrote.then_some(seq)) {
            self.record_view(branch, seq_of).await;
        }
        outcome
    }

    async fn record<T: serde::Serialize>(
        &self,
        branch: &BranchCtx,
        agent: Option<&str>,
        kind: EventKind,
        payload: &T,
    ) -> Result<(), String> {
        self.record_seq(branch, agent, kind, payload)
            .await
            .map(drop)
    }

    /// Records an event; its seq.
    async fn record_seq<T: serde::Serialize>(
        &self,
        branch: &BranchCtx,
        agent: Option<&str>,
        kind: EventKind,
        payload: &T,
    ) -> Result<i64, String> {
        let v = serde_json::to_value(payload).map_err(|e| format!("internal error: {e}"))?;
        let e = branch
            .append(&self.store, agent, kind, v)
            .await
            .map_err(|e| format!("internal error: recording event: {e:#}"))?;
        Ok(e.seq)
    }
}
