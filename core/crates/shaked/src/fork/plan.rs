//! Where a fork starts and what it carries over (the fork plan).

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

use super::changes::changes_kind;
pub(super) use super::changes::is_change;
use super::ForkError;
use crate::{
    model::*,
    recorded::{self, run_info, saves_at_except},
    world::{models::is_world_actor, Saves},
};

// Why a fork starts elsewhere than asked (`ForkPlan::reason`).
pub const REASON_EARLIER: &str = "a change targets an agent that finished earlier";
pub const REASON_CLI: &str = "Claude Code and Codex sessions restart from the agent's start";
pub const REASON_KIND: &str = "the agent's mode changes";
pub const REASON_TOOL: &str = "a tool call is redone from the model call that chose it";
pub const REASON_CONCURRENT: &str = "agents that ran at once all restart from the run's start";
pub const REASON_RUN_START: &str = "the run restarts from the first agent's start";
pub const REASON_FINISHED: &str = "the lead had finished the run: the fork starts at its finish";
pub const REASON_AGENT_RESTART: &str = "the agent restarts from its first start";
pub const REASON_LAST_MODEL_CALL: &str =
    "the step is redone from the agent's last model call before it";
/// The fork was asked at a world's own model call, made inside a step: it starts with that step.
pub const REASON_WORLD_CALL: &str =
    "a world's model call belongs to the step it was made in, so the fork starts with that step";

/// The world was not saved where the fork was asked (a part that saves at points changed since).
pub const REASON_SAVED: &str =
    "the world was last saved before an earlier step, so the fork starts there";
pub const REASON_NO_MODEL_CALL: &str =
    "the agent made no model call to resume from, so it restarts from its start";

/// Where a fork actually starts, and what it carries over.
#[derive(Debug, Clone, PartialEq)]
pub struct ForkPlan {
    pub start_seq: i64,
    /// `Some(at_seq)` when `start_seq != at_seq`.
    pub moved_from: Option<i64>,
    /// Why the fork point moved (None when it didn't).
    pub reason: Option<String>,
    pub agent_index: usize,
    /// Per part, the save the new branch's world starts from.
    pub restore: Saves,
    pub resume: Option<Vec<Value>>,
    /// Recorded handoff notes of the agents before `agent_index`, in chain order.
    pub notes: Vec<String>,
}

/// Where the new branch starts, and the resume messages it carries (open agents only).
struct Start {
    seq: i64,
    reason: Option<&'static str>,
    agent: usize,
    resume: Option<Vec<Value>>,
}

impl Start {
    /// A start at `seq` of agent `agent`, with no resume.
    fn at(seq: i64, reason: Option<&'static str>, agent: usize) -> Start {
        Start {
            seq,
            reason,
            agent,
            resume: None,
        }
    }
}

/// What a plan reads: the branch's events and team, and the fork point.
struct Planner<'a> {
    events: &'a [Event],
    team: &'a Team,
    at_seq: i64,
    at: &'a Event,
}

/// The index of the agent `id` in `team`.
fn index_of(team: &Team, id: &str) -> Option<usize> {
    team.agents.iter().position(|a| a.id == id)
}

impl<'a> Planner<'a> {
    /// The planner of a fork at `at_seq` of `events`.
    fn at(events: &'a [Event], team: &'a Team, at_seq: i64) -> Result<Planner<'a>, ForkError> {
        let at = events.iter().find(|e| e.seq == at_seq);
        let at = at.ok_or(ForkError::UnknownSeq)?;
        Ok(Planner {
            events,
            team,
            at_seq,
            at,
        })
    }

    /// An agent's real start: its first AgentStart (a resumed agent has a second one).
    fn first_start(&self, i: usize) -> Result<i64, ForkError> {
        let id = self.team.agents[i].id.as_str();
        self.events
            .iter()
            .find(|e| e.kind == EventKind::AgentStart && e.agent.as_deref() == Some(id))
            .map(|e| e.seq)
            .ok_or(ForkError::UnknownSeq)
    }

    /// The first AgentStart of any agent, or right after `RunStart` if none began.
    fn first_agent_start(&self) -> i64 {
        let first = self.events.iter().find(|e| e.kind == EventKind::AgentStart);
        first.map_or(self.at_seq + 1, |e| e.seq)
    }

    /// An agent has finished at `at_seq` once its AgentEnd is at or before it.
    fn finished(&self, i: usize) -> bool {
        let id = self.team.agents[i].id.as_str();
        self.events.iter().any(|e| {
            e.seq <= self.at_seq && e.kind == EventKind::AgentEnd && e.agent.as_deref() == Some(id)
        })
    }

    /// 1. The agent active at `at_seq`.
    fn active_agent(&self) -> Result<usize, ForkError> {
        let last_start = self
            .events
            .iter()
            .rev()
            .find(|e| e.seq <= self.at_seq && e.kind == EventKind::AgentStart);
        match (self.at.kind, last_start) {
            (EventKind::RunStart, _) | (_, None) => Ok(0),
            (_, Some(s)) => {
                let id = s.agent.clone().unwrap_or_default();
                index_of(self.team, &id).ok_or(ForkError::UnknownAgent(id))
            }
        }
    }

    /// Where the fork starts under `rules`, with no resume when that is the agent's first start.
    fn start_with(
        &self,
        rules: Rules,
        changes: &HashMap<String, Change>,
        changed: &[usize],
    ) -> Result<Start, ForkError> {
        let mut start = self.start_under(rules, changes, changed)?;
        if start.seq == self.first_start(start.agent).unwrap_or(i64::MIN) {
            start.resume = None;
        }
        Ok(start)
    }

    /// Where the fork starts under the run's runner. A concurrent run's agents all restart,
    /// from the first agent's start.
    fn start_under(
        &self,
        rules: Rules,
        changes: &HashMap<String, Change>,
        changed: &[usize],
    ) -> Result<Start, ForkError> {
        Ok(match rules.runner {
            Runner::Concurrent => Start::at(self.first_agent_start(), Some(REASON_CONCURRENT), 0),
            Runner::External => self.at_point(),
            Runner::Dispatch => self.dispatch_point(),
            Runner::Chain => self.start_for(changes, changed, self.active_agent()?)?,
        })
    }

    /// A dispatch run forks where asked: every agent restarts there in a new session with its
    /// recorded history (see `crate::dispatch`). Asked for `RunStart`, it starts at the first
    /// agent's start; asked for a step after the lead's finish, at the finish.
    fn dispatch_point(&self) -> Start {
        if self.at.kind == EventKind::RunStart {
            return Start::at(self.first_agent_start(), Some(REASON_RUN_START), 0);
        }
        let finish = self
            .events
            .iter()
            .find(|e| e.kind == EventKind::Finish && e.seq < self.at_seq);
        let seq = finish.map_or(self.at_seq, |f| f.seq);
        Start::at(seq, finish.map(|_| REASON_FINISHED), 0)
    }

    /// A run an external system drives forks where asked (right after `RunStart` when asked for
    /// it): how its agents resume is the system's choice.
    fn at_point(&self) -> Start {
        let after_start = self.at.kind == EventKind::RunStart;
        let seq = self.at_seq + i64::from(after_start);
        Start::at(seq, after_start.then_some(REASON_RUN_START), 0)
    }

    /// Restart agent `i` from its first start, for `reason`.
    fn restart(&self, i: usize, reason: &'static str) -> Result<Start, ForkError> {
        Ok(Start::at(self.first_start(i)?, Some(reason), i))
    }

    /// 2 to 4. Where the fork starts, given the agents `changed` and the active agent `a`.
    fn start_for(
        &self,
        changes: &HashMap<String, Change>,
        changed: &[usize],
        a: usize,
    ) -> Result<Start, ForkError> {
        // A change to an agent that has finished (including `a`): restart from it.
        let earliest = changed
            .iter()
            .copied()
            .filter(|&i| i < a || self.finished(i))
            .min();
        if let Some(e) = earliest {
            return self.restart(e, REASON_EARLIER);
        }
        let spec = &self.team.agents[a];
        // Mode change, or a CLI agent: restart the agent.
        if changes.get(&spec.id).is_some_and(|c| changes_kind(c, spec)) {
            return self.restart(a, REASON_KIND);
        }
        if spec.kind != AgentKind::Open {
            return self.restart(a, REASON_CLI);
        }
        match self.at.kind {
            // Begin with the first agent (or right after RunStart if it never began).
            EventKind::RunStart => {
                let seq = self.first_start(0).unwrap_or(self.at_seq + 1);
                Ok(Start::at(seq, Some(REASON_RUN_START), a))
            }
            EventKind::AgentStart => self.restart(a, REASON_AGENT_RESTART),
            _ => self.resume_from_model_call(a),
        }
    }

    /// The model call an open agent resumes from: the one at the fork point, else the agent's
    /// last one before it.
    fn model_call_before(&self, a: usize, agent_start: i64) -> Option<&Event> {
        if self.at.kind == EventKind::ModelCall {
            return Some(self.at);
        }
        let id = self.team.agents[a].id.as_str();
        self.events.iter().rev().find(|e| {
            e.seq <= self.at_seq
                && e.seq > agent_start
                && e.kind == EventKind::ModelCall
                && e.agent.as_deref() == Some(id)
        })
    }

    /// 4. Open agent: resume from a model call (its request's messages), else restart.
    fn resume_from_model_call(&self, a: usize) -> Result<Start, ForkError> {
        let agent_start = self.first_start(a)?;
        let model_call = self.model_call_before(a, agent_start);
        let resumable = model_call
            .and_then(|m| Some((m.seq, m.payload["request"]["messages"].as_array()?.clone())));
        let Some((seq, messages)) = resumable else {
            return Ok(Start::at(agent_start, Some(REASON_NO_MODEL_CALL), a));
        };
        let why = match self.at.kind {
            EventKind::ModelCall => None,
            EventKind::ToolCall | EventKind::Handoff => Some(REASON_TOOL),
            _ => Some(REASON_LAST_MODEL_CALL),
        };
        Ok(Start {
            resume: Some(messages),
            ..Start::at(seq, why, a)
        })
    }

    /// 6. The recorded handoff notes of the agents before `a`.
    fn handoff_notes(&self, a: usize, start_seq: i64) -> Result<Vec<String>, ForkError> {
        self.team.agents[..a]
            .iter()
            .map(|spec| {
                self.events
                    .iter()
                    .rev()
                    .find(|e| {
                        e.seq < start_seq
                            && e.kind == EventKind::Handoff
                            && e.agent.as_deref() == Some(spec.id.as_str())
                    })
                    .and_then(|e| e.payload["note"].as_str().map(str::to_string))
                    .ok_or_else(|| ForkError::MissingHandoff(spec.id.clone()))
            })
            .collect()
    }
}

/// The agents (by index) that `changes` really changes; an unknown agent is an error.
fn changed_agents(team: &Team, changes: &HashMap<String, Change>) -> Result<Vec<usize>, ForkError> {
    let mut changed = vec![];
    for (id, c) in changes {
        let i = index_of(team, id).ok_or_else(|| ForkError::UnknownAgent(id.clone()))?;
        if is_change(c, &team.agents[i]) {
            changed.push(i);
        }
    }
    Ok(changed)
}

/// Plans a fork of a branch (its full event list, `team` its effective team) at `at_seq`.
/// Empty changes (no field set, or only the agent's current kind) are ignored.
pub fn plan_fork(
    events: &[Event],
    team: &Team,
    at_seq: i64,
    changes: &HashMap<String, Change>,
) -> Result<ForkPlan, ForkError> {
    plan_fork_with(events, team, at_seq, changes, &BTreeSet::new())
}

/// `plan_fork` for a world whose `replayed` parts are restored by replay: they need no save. A
/// fork whose start has no save of some part (a part that saves at points, written since its last
/// save point) is planned again at the latest model call or agent start before that write
/// (`REASON_SAVED`); the run's start always restores, from the base.
pub fn plan_fork_with(
    events: &[Event],
    team: &Team,
    at_seq: i64,
    changes: &HashMap<String, Change>,
    replayed: &BTreeSet<String>,
) -> Result<ForkPlan, ForkError> {
    if team.agents.is_empty() {
        return Err(ForkError::UnknownAgent(String::new()));
    }
    let changed = changed_agents(team, changes)?;
    let rules = recorded::rules(events);
    // A world's own model call (or its reasoning) is part of the step it was made in.
    let step = step_of(events, at_seq);
    let mut asked = step.seq;
    loop {
        let planner = Planner::at(events, team, asked)?;
        let mut start = planner.start_with(rules.clone(), changes, &changed)?;
        let before_world = step.redo_world_calls(&mut start);
        let base = run_info(events).base;
        match saves_at_except(events, start.seq - 1, &base, replayed) {
            Ok(restore) => {
                let notes = match rules.handoff_required {
                    true => planner.handoff_notes(start.agent, start.seq)?,
                    // An agent's input then has the notes recorded before its start.
                    false => vec![],
                };
                let why = step.why_moved(asked, at_seq, before_world, &start);
                return Ok(finish_plan(start, at_seq, restore, notes, why));
            }
            Err(n) if save_failed(events, n.seq) => {
                return Err(ForkError::NoRestorePoint { seq: n.seq })
            }
            Err(n) => asked = saved_point_before(events, n.seq, asked)?,
        }
    }
}

/// The step a fork point belongs to: `seq`, and `first`, the first of the world's model calls
/// made in it (`seq` when it has none).
struct StepOf {
    seq: i64,
    first: i64,
}

impl StepOf {
    /// Moves `start` back to the step's first world call when it starts at the step itself, so
    /// the step's world calls are made again with it; true when it moved.
    fn redo_world_calls(&self, start: &mut Start) -> bool {
        let before = start.seq == self.seq && self.first != self.seq;
        if before {
            start.seq = self.first;
        }
        before
    }

    /// Why a fork asked at `at_seq`, planned from `asked`, moved: `REASON_SAVED` when it went back
    /// to an earlier save, else `REASON_WORLD_CALL` when the step's world calls moved it and the
    /// planner gave no reason of its own.
    fn why_moved(
        &self,
        asked: i64,
        at_seq: i64,
        before_world: bool,
        start: &Start,
    ) -> Option<&'static str> {
        let saved = (asked != self.seq).then_some(REASON_SAVED);
        let world = (self.seq != at_seq || before_world) && start.reason.is_none();
        saved.or(world.then_some(REASON_WORLD_CALL))
    }
}

/// The step `at_seq` belongs to: `at_seq` itself, or for an event of a world actor (a model call
/// a part or the scoring made inside a step, see `crate::world::models`) the first later event
/// of no world actor, the step it was made in. Its world calls are those right before it.
fn step_of(events: &[Event], at_seq: i64) -> StepOf {
    let world = |e: &Event| is_world_actor(e.agent.as_deref());
    let at = events.iter().position(|e| e.seq == at_seq);
    let i = at.and_then(|i| (i..events.len()).find(|&j| !world(&events[j])));
    let Some(i) = i else {
        return StepOf {
            seq: at_seq,
            first: at_seq,
        };
    };
    let calls = events[..i].iter().rev().take_while(|e| world(e)).last();
    StepOf {
        seq: events[i].seq,
        first: calls.map_or(events[i].seq, |e| e.seq),
    }
}

/// True when the write at `seq` was to be saved with its step and the save failed: no earlier
/// point stands in for it (a write to a part that saves at points has no save and no error).
fn save_failed(events: &[Event], seq: i64) -> bool {
    let at = events.iter().find(|e| e.seq == seq);
    at.is_some_and(|e| !e.payload["snapshot_error"].is_null())
}

/// The latest model call, agent start or run start before `seq` (the write that has no save),
/// earlier than the point `asked` already tried.
fn saved_point_before(events: &[Event], seq: i64, asked: i64) -> Result<i64, ForkError> {
    let point = |e: &&Event| {
        let kind = matches!(e.kind, EventKind::ModelCall | EventKind::AgentStart);
        let agents = kind && !is_world_actor(e.agent.as_deref());
        e.seq < seq.min(asked) && (agents || e.kind == EventKind::RunStart)
    };
    let found = events.iter().rev().find(point).map(|e| e.seq);
    found.ok_or(ForkError::NoRestorePoint { seq })
}

/// The plan of a fork asked at `at_seq` that starts at `start`; `saved` overrides the reason it
/// moved (it moved back to a save).
fn finish_plan(
    start: Start,
    at_seq: i64,
    restore: Saves,
    notes: Vec<String>,
    saved: Option<&'static str>,
) -> ForkPlan {
    let moved = start.seq != at_seq;
    ForkPlan {
        start_seq: start.seq,
        moved_from: moved.then_some(at_seq),
        reason: saved.or(start.reason).filter(|_| moved).map(str::to_string),
        agent_index: start.agent,
        restore,
        resume: start.resume,
        notes,
    }
}
