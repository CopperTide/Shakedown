//! The typed payloads of the events arenad writes without a payload struct of their own in
//! `model`: `RunStart`, `AgentStart`, `Wake`, `Idle`, `Pause`, `Resume` and `Reasoning`. Each is
//! written with [`BranchCtx::record`](crate::exec::BranchCtx::record) (or
//! [`Store::record`](crate::store::Store::record) before a branch has a context) and read with
//! [`Event::decode`]. Their JSON is the JSON written before they were typed.
//!
//! Every field reads leniently: a key that is absent, or holds something of another shape (a run
//! recorded by an older arenad), reads as its default, and the other keys still read, as each
//! key was read on its own before.

use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use super::{AgentKind, Event, EventKind, Runner, Team};
use crate::{reasoning::ReasoningKind, world::Saves};

/// A payload of events of kind `KIND`.
pub trait EventPayload: Serialize + DeserializeOwned + Default {
    const KIND: EventKind;
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
}

impl EventPayload for RunStartPayload {
    const KIND: EventKind = EventKind::RunStart;
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

impl EventPayload for AgentStartPayload {
    const KIND: EventKind = EventKind::AgentStart;
}

impl AgentStartPayload {
    /// The start of `spec`, an agent arenad runs.
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

impl EventPayload for WakePayload {
    const KIND: EventKind = EventKind::Wake;
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

impl EventPayload for IdlePayload {
    const KIND: EventKind = EventKind::Idle;
}

/// The payload of `EventKind::Pause` (`{}`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PausePayload {}

impl EventPayload for PausePayload {
    const KIND: EventKind = EventKind::Pause;
}

/// Why a save point was made: before a model call through the gateway.
pub const SAVE_BEFORE_MODEL_CALL: &str = "model_call";
/// Why a save point was made: a driver asked (`POST /api/branches/{id}/save`).
pub const SAVE_ASKED: &str = "asked";

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

impl EventPayload for SavePointPayload {
    const KIND: EventKind = EventKind::SavePoint;
}

/// The payload of `EventKind::Resume`: how long the pause lasted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResumePayload {
    #[serde(default, deserialize_with = "lenient")]
    pub paused_ms: i64,
}

impl EventPayload for ResumePayload {
    const KIND: EventKind = EventKind::Resume;
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

impl EventPayload for ReasoningPayload {
    const KIND: EventKind = EventKind::Reasoning;
}

#[cfg(test)]
mod tests;
