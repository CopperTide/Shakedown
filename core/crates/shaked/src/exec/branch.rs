//! A live branch's state: its world, team, clock, flow, records and the gate every step holds.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::MutexGuard;

use super::{records, BranchRecords, Lifecycle, Mailbox, Roster, ToolExec, CLOSED};
use crate::{
    clock::BranchClock,
    flow::{FlowControl, STOPPED},
    model::*,
    overhead::{Cat, Ledger},
    store::Store,
    ticker::Ticker,
    world::World,
};

/// One running branch: its world, team, world clock and per-agent handoff notes.
pub struct BranchCtx {
    pub branch_id: String,
    /// The run it is of, when it was started knowing it (else the store has it).
    pub run_id: Option<String>,
    pub world: Arc<World>,
    pub task_id: String,
    /// The branch's world clock (from 0 at the run's start, from the fork point on a fork).
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
    /// The Shakedown's time on this branch since its last event (see [`crate::overhead`]).
    pub overhead: Ledger,
    /// A finish held until the run's world time window ends (see [`crate::window`]).
    pub window: std::sync::Mutex<Option<crate::window::Held>>,
    /// Who the branch runs for: its records are written for this caller.
    pub caller: Arc<crate::auth::Caller>,
    /// Where its events and status are written and read.
    records: Arc<dyn BranchRecords>,
}

impl BranchCtx {
    /// A branch whose clock starts at 0 (at the speed `SHAKEDOWN_CLOCK_SPEED` sets), under the rules
    /// of runs recorded before runners (see [`Rules::legacy`]) until `with_rules` says otherwise.
    pub fn new(branch_id: String, world: Arc<World>, team: Team, task_id: String) -> BranchCtx {
        BranchCtx {
            branch_id,
            run_id: None,
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
            window: std::sync::Mutex::default(),
            caller: Arc::new(crate::auth::Caller::person()),
            records: Arc::new(records::NoRecords),
        }
    }

    /// The same branch run for `scope` (a new branch runs for the local owner until told).
    pub fn with_caller(mut self, caller: Arc<crate::auth::Caller>) -> BranchCtx {
        self.caller = caller;
        self
    }

    /// How long an agent token of this branch lives (unpaused time): a day, the local plan's
    /// longest branch.
    pub fn token_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(crate::auth::run_tokens::DEFAULT_TOKEN_SECS)
    }

    /// `agent`'s token on this branch, issued on first use.
    pub fn agent_token(&self, agent: &str) -> String {
        self.roster.tokens.ensure(agent, self.token_ttl())
    }

    /// The same branch, known to be of the run `run_id`.
    pub fn with_run_id(mut self, run_id: String) -> BranchCtx {
        self.run_id = Some(run_id);
        self
    }

    /// The same branch recording into `records`.
    pub fn with_records(mut self, records: Arc<dyn BranchRecords>) -> BranchCtx {
        self.records = records;
        self
    }

    /// The branch's records: its events and status.
    pub fn records(&self) -> &dyn BranchRecords {
        &*self.records
    }

    /// The store as this branch's scope (for what is not the branch's own events or status).
    pub fn scoped<'a>(&'a self, store: &'a Store) -> crate::store::Scoped<'a> {
        store.at(&self.caller)
    }

    /// The same branch under `rules`.
    pub fn with_rules(mut self, rules: Rules) -> BranchCtx {
        self.budget = crate::budget::Budget::new(rules.max_cost_usd);
        self.rules = rules;
        self
    }

    /// The same branch with `clock` as its world clock.
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
    pub async fn gate(&self) -> MutexGuard<'_, ()> {
        self.gate.lock().await
    }

    /// Records `payload` as an event of its kind on this branch (see `append`).
    pub async fn record<P: EventPayload>(
        &self,
        agent: Option<&str>,
        payload: &P,
    ) -> anyhow::Result<Event> {
        let payload = serde_json::to_value(payload)?;
        self.append(agent, P::KIND, payload).await
    }

    /// Records an event of this branch with the world clock paused: recording is Shakedown's work.
    pub async fn append(
        &self,
        agent: Option<&str>,
        kind: EventKind,
        payload: Value,
    ) -> anyhow::Result<Event> {
        let _paused = self.clock.paused();
        self.append_now(agent, kind, payload).await
    }

    /// Records an event of this branch, the world clock running on (for what happens outside the
    /// agents' and the world's work, like a driver's exit). It carries the overhead built up since
    /// the branch's last event; the time it takes is overhead of the kind `record`.
    pub async fn append_now(
        &self,
        agent: Option<&str>,
        kind: EventKind,
        payload: Value,
    ) -> anyhow::Result<Event> {
        let since = std::time::Instant::now();
        let built_up = self.overhead.take();
        let ev = self.records.append(agent, kind, payload, built_up).await;
        self.overhead.add_since(Cat::Record, since);
        ev
    }

    /// Pauses the branch: new tool calls and model calls wait and the world clock stands still
    /// until `resume`. Records `Pause`; false when the branch wasn't running.
    pub async fn pause(&self) -> anyhow::Result<bool> {
        if !self.flow.pause() {
            return Ok(false);
        }
        self.clock.pause();
        self.record(None, &PausePayload {}).await?;
        Ok(true)
    }

    /// Resumes a paused branch: the calls that wait go on and the clock runs again. Records
    /// `Resume` with how long the pause lasted (counted as Shakedown's time, not the run's); false
    /// when the branch wasn't paused.
    pub async fn resume(&self) -> anyhow::Result<bool> {
        Ok(self.resume_lasting().await?.is_some())
    }

    /// `resume`, saying how long the pause lasted (None: the branch wasn't paused).
    pub async fn resume_lasting(&self) -> anyhow::Result<Option<std::time::Duration>> {
        let Some(lasted) = self.flow.resume() else {
            return Ok(None);
        };
        let ms = lasted.as_secs_f64() * 1000.0;
        self.overhead.add(Cat::Pause, ms);
        self.clock.resume();
        self.roster.tokens.extend(lasted);
        let resumed = ResumePayload {
            paused_ms: ms.round() as i64,
        };
        self.record(None, &resumed).await?;
        Ok(Some(lasted))
    }

    /// Waits for the branch gate, counting the wait as Shakedown's overhead.
    pub(super) async fn gate_timed(&self) -> MutexGuard<'_, ()> {
        let since = std::time::Instant::now();
        let guard = self.gate.lock().await;
        self.overhead.add_since(Cat::Wait, since);
        guard
    }

    /// Lets a step in: waits while the branch is paused, then holds the gate (see `gate_timed`).
    /// Refused, with why, once the branch is stopped or its lead finished it.
    pub(super) async fn enter_step(&self) -> Result<MutexGuard<'_, ()>, &'static str> {
        self.flow.go().await.map_err(|_| STOPPED)?;
        let gate = self.gate_timed().await;
        if self.lifecycle.is_closed() {
            return Err(CLOSED);
        }
        Ok(gate)
    }
}
