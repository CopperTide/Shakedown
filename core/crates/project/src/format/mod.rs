//! The file formats of a project folder. Every format parses strictly: unknown fields, a wrong
//! `format:` line and inline credentials are refused with a JSON pointer to the value.

mod experiment;
mod observability;
mod project;
mod scenario;
mod setup;
mod view;
mod world;

pub use experiment::{Budget, ExperimentFile, Repeats};
pub use observability::{
    Alert, Behavior, HumanRule, Moment, ObsMode, ObsTool, Observability, Operator, Then,
};
pub use project::{AssistantSettings, Defaults, EditPolicy, Permissions, ProjectFile, SpendPolicy};
pub use scenario::{
    parse_duration_ms, ActionCost, Breach, Change, Check, CheckKind, ClockMode, ClockSpec, Event,
    Gate, ScenarioFile,
};
pub use setup::{AgentRuntime, AgentSpec, Effort, Reasoning, SetupFile, Team};
pub use view::{
    BarValue, BarsPanel, BoardPanel, Column, Count, FeedPanel, Field, Figure, Filter, Find, Join,
    KeyRef, KpiItem, KpisPanel, Layer, MapPanel, Panel, PathLayer, PointsLayer, Screen, Shown,
    Since, Sort, StyleRule, TimelinePanel, Tone, ViewFile,
};
pub use world::{Auth, Connector, ConnectorMode, DataSource, Flow, Person, WorldFile};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::{secrets, FormatError};

/// A semantic problem found after parsing: where it is and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    /// JSON pointer to the value.
    pub pointer: String,
    /// What is wrong.
    pub message: String,
}

impl Invalid {
    /// A problem at `pointer`.
    pub fn new(pointer: impl Into<String>, message: impl Into<String>) -> Self {
        Invalid {
            pointer: pointer.into(),
            message: message.into(),
        }
    }
}

/// A file format: its `format:` value and the checks serde cannot express.
pub trait Format: Serialize + DeserializeOwned + schemars::JsonSchema {
    /// The exact `format:` value files of this kind carry, e.g. `shakedown-world/3`.
    const FORMAT: &'static str;

    /// Checks beyond the shape, e.g. "exactly one of these fields".
    fn check(&self) -> Result<(), Invalid> {
        Ok(())
    }
}

/// The four kinds of item a project holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    World,
    Scenario,
    Setup,
    Experiment,
}

impl Kind {
    /// Every kind, in display order.
    pub const ALL: [Kind; 4] = [Kind::World, Kind::Scenario, Kind::Setup, Kind::Experiment];

    /// The folder this kind lives in.
    pub fn dir(self) -> &'static str {
        match self {
            Kind::World => "worlds",
            Kind::Scenario => "scenarios",
            Kind::Setup => "setups",
            Kind::Experiment => "experiments",
        }
    }

    /// The `format:` value of this kind's files.
    pub fn format(self) -> &'static str {
        match self {
            Kind::World => WorldFile::FORMAT,
            Kind::Scenario => ScenarioFile::FORMAT,
            Kind::Setup => SetupFile::FORMAT,
            Kind::Experiment => ExperimentFile::FORMAT,
        }
    }
}

/// Parses `text`, the contents of `file`, as format `T`.
pub fn parse<T: Format>(file: &str, text: &str) -> Result<T, FormatError> {
    let value = to_json(file, text)?;
    expect_format(file, &value, T::FORMAT)?;
    if let Some(pointer) = secrets::scan(&value).into_iter().next() {
        return Err(FormatError::new(file, pointer, CREDENTIAL));
    }
    let parsed: T = serde_path_to_error::deserialize(value).map_err(|e| shape_error(file, &e))?;
    parsed
        .check()
        .map_err(|i| FormatError::new(file, i.pointer, i.message))?;
    Ok(parsed)
}

/// Serializes a file as YAML. It goes through JSON first, so enums with data are written as plain
/// maps (`answers_after: {minutes: 15}`) rather than YAML tags, and read back the same way.
pub fn to_yaml<T: Serialize>(value: &T) -> Result<String, FormatError> {
    let json = serde_json::to_value(value).map_err(|e| FormatError::new("", "", e.to_string()))?;
    serde_yaml_ng::to_string(&json).map_err(|e| FormatError::new("", "", e.to_string()))
}

const CREDENTIAL: &str =
    "this looks like a credential. Keep it in the key store and refer to it by name \
                          (key: <name>) or by an environment variable (env: NAME)";

fn to_json(file: &str, text: &str) -> Result<Value, FormatError> {
    let yaml: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(text).map_err(|e| FormatError::new(file, "", e.to_string()))?;
    serde_json::to_value(yaml).map_err(|e| FormatError::new(file, "", e.to_string()))
}

fn expect_format(file: &str, value: &Value, want: &str) -> Result<(), FormatError> {
    match value.get("format").and_then(Value::as_str) {
        Some(found) if found == want => Ok(()),
        _ => Err(FormatError::new(
            file,
            "/format",
            format!("expected format: {want}"),
        )),
    }
}

fn shape_error(file: &str, err: &serde_path_to_error::Error<serde_json::Error>) -> FormatError {
    let mut pointer = to_pointer(err.path());
    let message = err.inner().to_string();
    if let Some(field) = unknown_field(&message) {
        if !pointer.ends_with(&format!("/{field}")) {
            pointer = format!("{pointer}/{}", secrets::escape(field));
        }
    }
    FormatError::new(file, pointer, message)
}

fn to_pointer(path: &serde_path_to_error::Path) -> String {
    use serde_path_to_error::Segment;
    path.iter()
        .filter_map(|segment| match segment {
            Segment::Seq { index } => Some(index.to_string()),
            Segment::Map { key } => Some(secrets::escape(key)),
            Segment::Enum { .. } | Segment::Unknown => None,
        })
        .map(|part| format!("/{part}"))
        .collect()
}

fn unknown_field(message: &str) -> Option<&str> {
    message.strip_prefix("unknown field `")?.split('`').next()
}
