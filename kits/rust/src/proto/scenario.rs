//! A run's scenario: the disruptions a benchmark offers a scenario builder (`describe.
//! scenario_options`), and the choices a run or suite makes of them. See docs/protocol.md,
//! "A run's scenario".
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// What a benchmark offers a scenario builder: the tasks a scenario may start from and the
/// disruptions it may add.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScenarioOptions {
    /// The task ids a scenario may start from.
    pub base: Vec<String>,
    pub disruptions: Vec<DisruptionOption>,
    /// Per part, the setup call shaked adds to a task that has none for that part, to carry the
    /// part's disruptions (the part's scenario tool, with its base arguments).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub setup: BTreeMap<String, SetupTemplate>,
}

/// The setup call shaked makes on `part` to carry disruptions when a task has none for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupTemplate {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

/// One disruption a builder may add: shown as `label` and `description`, it is `template` with
/// its `{param}` placeholders filled in, appended to the setup args of `part`'s scenario call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisruptionOption {
    pub id: String,
    pub label: String,
    pub description: String,
    pub part: String,
    /// True: the builder offers it selected.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
    #[serde(default)]
    pub params: Vec<ScenarioParam>,
    /// A disruption (`{id, when, do, text}`); string values may contain `{param}`. A string that
    /// is exactly one placeholder takes the param's value as it is (a number stays a number).
    pub template: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    Int,
    Choice,
}

/// A knob of a disruption: an integer (within `min` and `max`) or one of `choices`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioParam {
    pub name: String,
    pub label: String,
    pub kind: ParamKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    pub default: Value,
}

impl ScenarioParam {
    /// An integer knob from `min` to `max`, `default` unless chosen.
    pub fn int(name: &str, label: &str, (min, max, default): (i64, i64, i64)) -> ScenarioParam {
        ScenarioParam {
            name: name.into(),
            label: label.into(),
            kind: ParamKind::Int,
            choices: None,
            min: Some(min),
            max: Some(max),
            default: Value::from(default),
        }
    }

    /// One of `choices`, the first unless chosen.
    pub fn choice(name: &str, label: &str, choices: &[&str]) -> ScenarioParam {
        ScenarioParam {
            name: name.into(),
            label: label.into(),
            kind: ParamKind::Choice,
            choices: Some(choices.iter().map(|c| Value::from(*c)).collect()),
            min: None,
            max: None,
            default: Value::from(choices[0]),
        }
    }
}

/// The scenario a run or suite asks for: `{disruptions: [{id, params}], timed?, generators?,
/// clock?}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScenarioRequest {
    #[serde(default)]
    pub disruptions: Vec<ChosenDisruption>,
    /// Disruptions given as they are (a project scenario's timed events), each for the setup call
    /// of its part.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timed: Vec<TimedDisruption>,
    /// Background generators, given to every part's setup call as `args.generators`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generators: Vec<Value>,
    /// A stepped world clock for the run; absent: the clock runs on wall time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<SteppedClock>,
    /// The world's data every part starts from (docs/protocol.md, "World data"), set by shaked
    /// when it deploys a project world that has some.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_data: Option<Value>,
}

/// A disruption for `part`'s setup call, as it is: `{id, at, known_at, at_ms, known_at_ms, do,
/// text}` for a world-time event (see docs/protocol.md, "Scheduled disruptions").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedDisruption {
    pub part: String,
    pub disruption: Value,
}

/// A stepped world clock (see docs/protocol.md, "Stepped clock"), in world ms: what agents' tool
/// calls cost, how long a quiet team goes before the lead gets a look, and where the run ends.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteppedClock {
    #[serde(default)]
    pub read_ms: i64,
    #[serde(default)]
    pub write_ms: i64,
    /// Per tool, its own cost.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<String, i64>,
    /// The longest a quiet team goes before the lead is woken for a look; absent: no looks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch_ms: Option<i64>,
    /// The branch clock the run ends at; absent: it ends when the lead finishes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizon_ms: Option<i64>,
    /// The branch clock the team starts at: before it the world runs with nobody deciding (its
    /// parts' own routine automation), through every wake, as after an early finish. Absent: the
    /// team starts at clock 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_start_ms: Option<i64>,
}

/// One disruption chosen: an option's `id` and the values of its params (defaults fill the rest).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChosenDisruption {
    pub id: String,
    #[serde(default)]
    pub params: Map<String, Value>,
}
