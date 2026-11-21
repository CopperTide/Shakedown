//! `scenarios/<id>.yaml`: an incident or test inside one world.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Format, Invalid};

/// A scenario: inputs (task, starting changes, injected events), gates and checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFile {
    /// Always `shakedown-scenario/1`.
    pub format: String,
    /// The scenario's display name.
    pub label: String,
    /// The world it happens in.
    pub world: String,
    /// The ticket every setup gets.
    pub task: String,
    /// Records changed for this scenario only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<Change>,
    /// Failures and disruptions injected during the run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    /// Rules checked on every agent call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gates: Vec<Gate>,
    /// What must be true at the end.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Check>,
}

impl Format for ScenarioFile {
    const FORMAT: &'static str = "shakedown-scenario/1";

    fn check(&self) -> Result<(), Invalid> {
        for (i, event) in self.events.iter().enumerate() {
            if event.at.is_some() == event.after_calls.is_some() {
                let message =
                    "an event fires either at a world time or after a number of agent calls";
                return Err(Invalid::new(format!("/events/{i}"), message));
            }
        }
        Ok(())
    }
}

/// A record changed at the start of the scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Change {
    /// `<connector>.<record path>`, e.g. `erp.sales_order/SO-1001.due`.
    pub record: String,
    /// The value it starts with.
    pub value: Value,
}

/// A failure or disruption injected during the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Event {
    /// Display name.
    pub name: String,
    /// The world time it fires, e.g. `07:30`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// Fires after this many agent calls instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_calls: Option<u32>,
    /// Record changes it makes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Value>,
    /// The database a database fault acts on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db: Option<String>,
    /// The fault, e.g. `pool_exhausted` or `lock_contention`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<String>,
    /// The fault's parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// A rule checked on every agent call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    /// The rule in plain language.
    pub rule: String,
    /// The tool it applies to, e.g. `erp.create_purchase_order`.
    pub scope: String,
    /// What happens when a call breaks it.
    pub on_breach: Breach,
}

/// What a gate does to a call that breaks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Breach {
    /// Refuse the call.
    Block,
    /// Allow it and flag it for review.
    Flag,
}

/// A check on the final state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// The check in plain language.
    pub text: String,
    /// A goal or a safety rule.
    pub kind: CheckKind,
    /// The query it was understood as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// The run ends as soon as this holds.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ends_run: bool,
}

/// What a check is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    /// What a good outcome looks like.
    Goal,
    /// Must hold no matter how the goal is reached.
    Safety,
}
