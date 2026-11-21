//! `shakedown.yaml`: the project's name, defaults and what agents may do without asking.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Format;

/// The project file at the root of the folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    /// Always `shakedown-project/1`.
    pub format: String,
    /// Shown in the sidebar and on reports.
    pub name: String,
    /// Used when a new experiment doesn't say otherwise.
    #[serde(default)]
    pub defaults: Defaults,
    /// What the assistant and MCP clients may do without asking.
    #[serde(default)]
    pub permissions: Permissions,
}

impl Format for ProjectFile {
    const FORMAT: &'static str = "shakedown-project/1";
}

/// Defaults for new experiments.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// The world new experiments start in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    /// The cost cap per run, in US dollars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_cap_usd: Option<f64>,
}

/// What agents may do without asking.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// Edits to project files.
    #[serde(default)]
    pub edit: EditPolicy,
    /// Starting runs and spending money.
    #[serde(default)]
    pub spend: SpendPolicy,
}

/// How edits to project files are made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditPolicy {
    /// Show a diff; write only once confirmed.
    #[default]
    DiffFirst,
    /// Write, then report what changed.
    ApplyThenTell,
}

/// How runs and spending are approved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpendPolicy {
    /// Every run or spend asks first.
    #[default]
    AlwaysAsk,
    /// Allowed without asking up to a daily cap.
    UnderCap {
        /// The daily cap in US dollars.
        usd_per_day: f64,
    },
}
