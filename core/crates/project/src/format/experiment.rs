//! `experiments/<id>.yaml`: one scenario against agent setups.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Format, Invalid};

/// An experiment: a scenario, the setups compared, repeats and budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExperimentFile {
    /// Always `shakedown-experiment/1`.
    pub format: String,
    /// The experiment's display name.
    pub label: String,
    /// The world, which must be the scenario's world.
    pub world: String,
    /// The scenario every run plays.
    pub scenario: String,
    /// The setups compared.
    pub setups: Vec<String>,
    /// How many runs each setup gets.
    pub repeats: Repeats,
    /// Spending caps.
    #[serde(default)]
    pub budget: Budget,
    /// The experiment this one was branched from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

impl Format for ExperimentFile {
    const FORMAT: &'static str = "shakedown-experiment/1";

    fn check(&self) -> Result<(), Invalid> {
        if self.setups.is_empty() {
            return Err(Invalid::new(
                "/setups",
                "an experiment compares at least one setup",
            ));
        }
        if self.repeats.per_setup == 0 {
            return Err(Invalid::new(
                "/repeats/per_setup",
                "each setup needs at least one run",
            ));
        }
        Ok(())
    }
}

/// Runs per setup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Repeats {
    /// Runs per setup.
    pub per_setup: u32,
    /// Run n of every setup uses the same seed.
    #[serde(default = "yes")]
    pub paired_seeds: bool,
}

fn yes() -> bool {
    true
}

/// Spending caps in US dollars.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// The cap for one run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_run_usd: Option<f64>,
    /// The cap for one deployment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_deployment_usd: Option<f64>,
}
