//! The typed payload of every event kind, written with
//! [`BranchCtx::record`](crate::exec::BranchCtx::record) and read with [`Event::decode`]. Their
//! JSON is the JSON recorded runs hold.
//!
//! The payloads typed after their events were first written (`RunStart`, `AgentStart`, `Wake`,
//! `Idle`, `Resume` and `Reasoning`) read every field leniently: a key that is absent, or holds
//! something of another shape (a run recorded by an older shaked), reads as its default, and the
//! other keys still read, as each key was read on its own before. The others read as they
//! always have.

use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use super::{AgentKind, Event, EventKind, Runner, Team};
use crate::{reasoning::ReasoningKind, world::Saves};

/// A payload of events of kind `KIND`.
pub trait EventPayload: Serialize + DeserializeOwned + Default {
    const KIND: EventKind;
}

/// Each payload's kind, one line per kind.
macro_rules! payload_kind {
    ($($payload:ty => $kind:ident,)+) => {
        $(impl EventPayload for $payload {
            const KIND: EventKind = EventKind::$kind;
        })+
    };
}

payload_kind! {
    RunStartPayload => RunStart,
    AgentStartPayload => AgentStart,
    ModelCallPayload => ModelCall,
    ToolCallPayload => ToolCall,
    HandoffPayload => Handoff,
    MessagePayload => Message,
    AgentEndPayload => AgentEnd,
    CheckPayload => Check,
    WorldEventPayload => WorldEvent,
    WorldViewPayload => WorldView,
    PausePayload => Pause,
    ResumePayload => Resume,
    DelegationPayload => Delegation,
    ReportPayload => Report,
    WakePayload => Wake,
    IdlePayload => Idle,
    FinishPayload => Finish,
    ReasoningPayload => Reasoning,
    SavePointPayload => SavePoint,
}

impl Event {
    /// This event's payload as `P`, when the event is of `P`'s kind (a payload that isn't an
    /// object reads as `P`'s defaults).
    pub fn decode<P: EventPayload>(&self) -> Option<P> {
        (self.kind == P::KIND).then(|| P::deserialize(&self.payload).unwrap_or_default())
    }
}

/// A field's value, or its default when it has another shape.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(T::deserialize(v).unwrap_or_default())
}

/// The payload of `EventKind::RunStart`: what a run is of, its world as it started, and its
/// rules. Runs recorded before integrations have none of the world's keys; runs recorded before
/// runners have no `runner` (see [`Rules::of`](super::Rules::of)).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunStartPayload {
    #[serde(default, deserialize_with = "lenient")]
    pub task_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub team: Option<Team>,
    #[serde(default, deserialize_with = "lenient")]
    pub benchmark: Option<String>,
    /// The world's parts with their integrations.
    #[serde(default, deserialize_with = "lenient")]
    pub world: Option<Vec<parts::proto::WorldPart>>,
    #[serde(default, deserialize_with = "lenient")]
    pub seed: u64,
    /// The seed the base save was made with.
    #[serde(default, deserialize_with = "lenient")]
    pub base_seed: u64,
    #[serde(default, deserialize_with = "lenient")]
    pub fingerprints: Map<String, Value>,
    /// `{part: save}` of the base; only parts that keep saves.
    #[serde(default, deserialize_with = "lenient")]
    pub saves: Option<Saves>,
    #[serde(default, deserialize_with = "lenient")]
    pub runner: Option<Runner>,
    /// `required` or `optional` ([`Rules::handoff_word`](super::Rules::handoff_word)).
    #[serde(default, deserialize_with = "lenient")]
    pub handoff: Option<String>,
    /// What `World::systems` says: each part's system.
    #[serde(default, deserialize_with = "lenient")]
    pub systems: Map<String, Value>,
    /// What `World::tool_info` says: `{name: {effect, part?}}`.
    #[serde(default, deserialize_with = "lenient")]
    pub tools: Map<String, Value>,
    /// The expanded scenario ([`Scenario`](crate::scenario::Scenario)), when the run has one;
    /// kept as recorded, so what reads it repeats an older shape as it was.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub scenario: Option<Value>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_cost_usd: Option<f64>,
    /// The Code connection whose driver Shakedown launched for the run.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub connection: Option<String>,
    /// The unix ms of the world's clock 0 (`world_time.epoch_ms` its parts get); runs recorded
    /// before world time have none (their epoch is `RunStart.at`).
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub world_epoch_ms: Option<i64>,
    /// The run's world time window in minutes, when it has one (`crate::window`).
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub world_minutes: Option<f64>,
}

impl RunStartPayload {
    /// The `RunStart` of a branch's events, when it has one.
    pub fn of(events: &[Event]) -> Option<RunStartPayload> {
        events.iter().find_map(Event::decode)
    }
}

/// The payload of `EventKind::AgentStart`. `external` is set for an agent an external system
/// drives.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentStartPayload {
    #[serde(default, deserialize_with = "lenient")]
    pub name: String,
    #[serde(default, deserialize_with = "lenient")]
    pub kind: Option<AgentKind>,
    #[serde(default, deserialize_with = "lenient")]
    pub model: Option<String>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub provider: Option<String>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub external: bool,
}

impl AgentStartPayload {
    /// The start of `spec`, an agent shaked runs.
    pub fn of(spec: &super::AgentSpec) -> AgentStartPayload {
        AgentStartPayload {
            name: spec.name.clone(),
            kind: Some(spec.kind),
            model: spec.model.clone(),
            provider: spec.provider.clone(),
            external: false,
        }
    }
}

/// The payload of `EventKind::Wake`: why a dispatch run's agent was woken (`start`, `mail`,
/// `idle`, `resume`), the seqs of the mail it was woken with, its turn's input, and on an idle
/// wake which one it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WakePayload {
    #[serde(default, deserialize_with = "lenient")]
    pub reason: String,
    #[serde(default, deserialize_with = "lenient")]
    pub mail: Vec<i64>,
    #[serde(default, deserialize_with = "lenient")]
    pub input: String,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub idle_wake: Option<u32>,
}

/// The payload of `EventKind::Idle`: a dispatch run's agent ended its turn, with what it said.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IdlePayload {
    /// Always written; `None` reads a payload without it, which is not a failed turn.
    #[serde(default, deserialize_with = "lenient")]
    pub ok: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    pub text: String,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub error: Option<String>,
}

/// The payload of `EventKind::Pause` (`{}`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PausePayload {}

/// Why a save point was made: before a model call through the gateway.
pub const SAVE_BEFORE_MODEL_CALL: &str = "model_call";
/// Why a save point was made: a driver asked (`POST /api/branches/{id}/save`).
pub const SAVE_ASKED: &str = "asked";
/// Why a save point was made: the task's setup (and the links it set off) wrote, so a fork where
/// the agents start has the world they started in.
pub const SAVE_AFTER_SETUP: &str = "setup";

/// The payload of `EventKind::SavePoint`: why the parts that save at points were saved, their new
/// saves, the first save that failed, and how long the world clock stood still for it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavePointPayload {
    pub reason: String,
    pub saves: Saves,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub paused_ms: i64,
}

/// The payload of `EventKind::Resume`: how long the pause lasted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResumePayload {
    #[serde(default, deserialize_with = "lenient")]
    pub paused_ms: i64,
}

/// The payload of `EventKind::Reasoning`: one thing an agent thought or said it would do, where
/// it came from (`crate::reasoning::SOURCE_*`), the model call it came with, and whether its text
/// was cut.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReasoningPayload {
    #[serde(default, deserialize_with = "lenient")]
    pub text: String,
    #[serde(default, deserialize_with = "lenient")]
    pub kind: ReasoningKind,
    #[serde(default, deserialize_with = "lenient")]
    pub source: String,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub model_call_seq: Option<i64>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub truncated: bool,
}

/// Payload of `EventKind::ToolCall`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolCallPayload {
    pub name: String,
    pub args: Value,
    pub result: Value,
    pub error: Option<String>,
    pub reads: Value,
    pub writes: Value,
    /// Keys the call changed for its part's own bookkeeping (`_disruption/<id>`): saved like a
    /// write, but not one (docs/protocol.md, "Ids and keys").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub internal: Vec<String>,
    /// `saves["retail"]` when that part was saved, so the old UIs keep working. Runs recorded
    /// before integrations have only this (read as `saves: {"retail": snapshot}`).
    pub snapshot: Option<String>,
    /// Set when the write committed but saving it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_error: Option<String>,
    pub tool_call_id: Option<String>,
    /// The world part the tool ran on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// `{part: save}` of the parts saved after this call (None when nothing was saved).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saves: Option<std::collections::BTreeMap<String, String>>,
    /// The world clock when the call ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_ms: Option<i64>,
    /// How long the world's clock was paused for this step's saves (None when nothing was saved).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_ms: Option<i64>,
    /// How long the tool ran in the world, gate checks and saves left out (None when it reached
    /// no part, and in calls recorded before durations were kept).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
}

/// Payload of `EventKind::WorldEvent`: something a part reported when its clock advanced.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldEventPayload {
    pub part: String,
    pub kind: String,
    pub text: String,
    pub data: Value,
    pub clock_ms: i64,
}

/// Payload of `EventKind::WorldView`: the world's view the benchmark drew after the step at
/// `seq_of`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldViewPayload {
    pub seq_of: i64,
    pub view: Value,
}

/// Payload of `EventKind::ModelCall`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelCallPayload {
    pub request: Value,
    pub response: Value,
    pub usage: Value,
    pub cost_usd: Option<f64>,
    pub cost_source: Option<String>,
    /// The provider that answered (absent in calls recorded before providers: OpenRouter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The provider's own response, when it differs from the canonical `response` (a provider
    /// that doesn't speak chat completions); absent when `response` is what it sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_response: Option<Value>,
    /// How long the provider took to answer, in milliseconds (absent in calls recorded before
    /// it was kept, and in calls a client reported).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
}

/// Payload of `EventKind::AgentEnd`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentEndPayload {
    pub ok: bool,
    pub error: Option<String>,
    pub tokens: Option<i64>,
    /// The input and output tokens of `tokens` (absent in runs recorded before they were kept).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub cost_source: Option<String>,
    pub seconds: Option<f64>,
}

/// Payload of `EventKind::Handoff`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HandoffPayload {
    pub note: String,
    /// The agent the note is for; absent: for whoever comes next.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

/// Payload of `EventKind::Message`: one agent's message to another, or to all (`to` = `*`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessagePayload {
    pub to: String,
    pub text: String,
}

/// Payload of `EventKind::Delegation`: the lead gives agent `to` its `task`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DelegationPayload {
    pub to: String,
    pub task: String,
}

/// Payload of `EventKind::Report`: an agent's report to the lead (`to`). `turn_end` is set when
/// the agent ended a turn without reporting and shaked passed on what it said last.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportPayload {
    pub to: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub turn_end: bool,
}

/// Payload of `EventKind::Finish`: the lead's summary of the run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FinishPayload {
    pub summary: String,
}

/// Payload of `EventKind::Check`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CheckPayload {
    pub passed: bool,
    /// The gold save (of the first part) the world was compared with.
    pub gold_db: Option<String>,
    /// Keys (`<table>/<id>`) whose documents differ from the gold save, sorted; prefixed
    /// `part:` when the world has more than one part.
    pub differing_rows: Option<Vec<String>>,
    /// The benchmark's named checks, for a benchmark that scores the end state itself. Then
    /// `differing_rows` lists the checks that failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<Vec<parts::proto::NamedCheck>>,
    /// `{part: save}` of every part's final state, saved before its instance stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_saves: Option<std::collections::BTreeMap<String, String>>,
    /// Parts that keep no gold save (parts with replay), so they could not be compared with
    /// the task's expected result.
    #[serde(default)]
    pub unchecked: Vec<String>,
}

#[cfg(test)]
mod tests;
