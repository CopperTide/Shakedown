//! A world's observability and on-call: the monitoring tools runs report to, the alert rules that
//! fire inside runs, the simulated people who answer pages, and what counts as needing a human.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Auth;

/// Observability and on-call for a world.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Observability {
    /// Monitoring, alerting and incident tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ObsTool>,
    /// Alert rules that fire inside runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<Alert>,
    /// Simulated on-call people.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operators: Vec<Operator>,
    /// What counts as needing a human, and what the run does then.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub human: Vec<HumanRule>,
}

/// One tool: Datadog, Grafana, PagerDuty, an OpenTelemetry collector…
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObsTool {
    pub id: String,
    /// The product, e.g. `datadog`, `pagerduty`, `otel-collector`.
    pub product: String,
    /// Real (receives the sandbox's telemetry, tagged with the run id) or a built-in stand-in.
    pub mode: ObsMode,
    /// The export a stand-in is seeded from, e.g. `monitors.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seeded_from: Option<String>,
    /// Where the credential comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
}

/// How runs use an observability tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ObsMode {
    Real,
    StandIn,
}

/// An alert rule that fires inside runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Alert {
    pub name: String,
    /// The condition, e.g. `approvals.pending > 3`.
    pub when: String,
    /// The operator or channel it pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routes_to: Option<String>,
}

/// A simulated on-call person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Operator {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub behavior: Behavior,
}

/// How a simulated operator responds to pages and requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Behavior {
    /// Acknowledges and answers after this many world minutes.
    AnswersAfter { minutes: u32 },
    /// Approves every request.
    Approves,
    /// Refuses every request.
    Refuses,
    /// Never answers, to test what agents do without a person.
    NeverAnswers,
}

/// A moment that counts as needing a human, and what the run does when it happens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HumanRule {
    pub moment: Moment,
    pub then: Then,
}

/// The moments Shakedown intercepts and reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Moment {
    Paged,
    Approval,
    GateNeedsPerson,
    AgentStuck,
    MessageToPerson,
}

/// What the run does at such a moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Then {
    Continue,
    Pause,
    Fail,
}
