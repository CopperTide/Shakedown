//! `scenarios/<id>.yaml`: an incident or test inside one world.

use std::collections::BTreeMap;

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
    /// The task of the world's package this scenario plays, e.g. `t1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// How world time moves. Without it the clock runs on wall time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<ClockSpec>,
    /// Records changed for this scenario only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<Change>,
    /// Failures and disruptions injected during the run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    /// Background generators (sick-call rates, weather cells), passed to every part's setup as
    /// they are. The parts expand them from the seed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generators: Vec<Value>,
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
        if let Some(clock) = &self.clock {
            clock.check()?;
        }
        for (i, event) in self.events.iter().enumerate() {
            let timed = event.at.is_some() || event.known_at.is_some();
            if timed == event.after_calls.is_some() {
                let message =
                    "an event fires either at a world time or after a number of agent calls";
                return Err(Invalid::new(format!("/events/{i}"), message));
            }
            if event.do_.is_some() && event.part.is_none() {
                let message = "name the part that carries out `do`";
                return Err(Invalid::new(format!("/events/{i}/part"), message));
            }
        }
        Ok(())
    }
}

/// How world time moves during a run.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClockSpec {
    /// `wall` (the clock runs while agents work) or `stepped` (it moves when the team acts or
    /// waits).
    #[serde(default)]
    pub mode: ClockMode,
    /// The world time each agent tool call costs in a stepped run.
    #[serde(default)]
    pub action_cost: ActionCost,
    /// The longest a quiet stepped run goes before the lead gets a look, e.g. `60m`, or `none`.
    /// Defaults to `60m`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch: Option<String>,
    /// The world time the run ends at, e.g. `23:59` on the task's day or an ISO timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizon: Option<String>,
}

impl ClockSpec {
    /// The default epoch when none is set.
    pub const DEFAULT_EPOCH: &'static str = "60m";

    /// The epoch in ms, or `None` when it is turned off with `epoch: none`.
    ///
    /// # Errors
    /// The epoch is not a duration.
    pub fn epoch_ms(&self) -> Result<Option<i64>, String> {
        match self.epoch.as_deref().unwrap_or(Self::DEFAULT_EPOCH) {
            "none" => Ok(None),
            text => parse_duration_ms(text).map(Some),
        }
    }

    /// Whether the clock is stepped.
    pub fn is_stepped(&self) -> bool {
        self.mode == ClockMode::Stepped
    }

    fn check(&self) -> Result<(), Invalid> {
        let bad = |pointer: &str, e: String| Invalid::new(format!("/clock{pointer}"), e);
        self.epoch_ms().map_err(|e| bad("/epoch", e))?;
        let cost = &self.action_cost;
        parse_duration_ms(&cost.read).map_err(|e| bad("/action_cost/read", e))?;
        parse_duration_ms(&cost.write).map_err(|e| bad("/action_cost/write", e))?;
        for (tool, text) in &cost.tools {
            let pointer = format!("/action_cost/tools/{}", crate::secrets::escape(tool));
            parse_duration_ms(text).map_err(|e| bad(&pointer, e))?;
        }
        Ok(())
    }
}

/// How the clock moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClockMode {
    /// World time runs with wall time while agents work.
    #[default]
    Wall,
    /// World time moves only when agents act (by `action_cost`) or the team waits.
    Stepped,
}

/// The world time an agent tool call costs, by its effect, with per-tool overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActionCost {
    /// A read call, e.g. `0m`.
    #[serde(default = "default_read")]
    pub read: String,
    /// A write call, e.g. `2m`.
    #[serde(default = "default_write")]
    pub write: String,
    /// Per tool, overriding its effect's cost, e.g. `plan_recovery: 5m`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<String, String>,
}

impl Default for ActionCost {
    fn default() -> Self {
        ActionCost {
            read: default_read(),
            write: default_write(),
            tools: BTreeMap::new(),
        }
    }
}

fn default_read() -> String {
    "0m".into()
}

fn default_write() -> String {
    "2m".into()
}

/// Parses a duration such as `250ms`, `90s`, `2m` or `1h` into milliseconds.
///
/// # Errors
/// The text is not a whole, non-negative number followed by `ms`, `s`, `m` or `h`.
pub fn parse_duration_ms(text: &str) -> Result<i64, String> {
    let bad = || {
        format!("`{text}` is not a duration: write a whole number and a unit, like 2m, 90s or 1h")
    };
    let t = text.trim();
    let split = t.find(|c: char| !c.is_ascii_digit()).ok_or_else(bad)?;
    let (number, unit) = t.split_at(split);
    let n: i64 = number.parse().map_err(|_| bad())?;
    let scale = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        _ => return Err(bad()),
    };
    n.checked_mul(scale).ok_or_else(bad)
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
    /// The world time it happens, e.g. `07:30` on the task's day or an ISO timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// The world time the world learns of it; defaults to `at`. Earlier is a forecast, later is
    /// found out late. Parts never reveal the event before this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_at: Option<String>,
    /// The part that carries the event out, e.g. `driver`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// What the part does, e.g. `{kind: capacity_program, station: ORD}`. Passed to the part's
    /// setup as it is.
    #[serde(default, rename = "do", skip_serializing_if = "Option::is_none")]
    pub do_: Option<Value>,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// The check in plain language.
    pub text: String,
    /// A goal or a safety rule.
    pub kind: CheckKind,
    /// The query it was understood as: plain text, or a structured query such as
    /// `{part: driver, key: kpis, field: cost_vs_baseline, op: lt, value: 1.0}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<Value>,
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
