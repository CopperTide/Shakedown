use chrono::{DateTime, Utc};
use parts::proto::Mode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::secrets::Headers;

mod agent_in;
mod payloads;
pub mod team;

pub use payloads::*;
pub use team::TeamError;

/// A fieldless enum whose variants each have one name, used everywhere it is written or read:
/// in serde, `as_str`, `parse`, and the store. `ALL` lists the variants in order.
macro_rules! named_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $($(#[$vmeta:meta])* $variant:ident = $text:literal,)+
        }
    ) => {
        $(#[$meta])*
        #[derive(Serialize, Deserialize)]
        pub enum $name {
            $($(#[$vmeta])* #[serde(rename = $text)] $variant,)+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(&self) -> &'static str {
                match self {
                    $($name::$variant => $text,)+
                }
            }

            pub fn parse(s: &str) -> Option<$name> {
                match s {
                    $($text => Some($name::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    ClaudeCode,
    Codex,
    Open,
    /// Driven by an external system (a design's own code), not by shaked: it reaches the world
    /// through its MCP endpoint and its models through the gateway.
    External,
    /// An agent deployed anywhere, behind a URL: shaked POSTs it the run (the task, its MCP
    /// endpoint and a token) and it answers when it is done (see `crate::agents::http`).
    Http,
}

/// A team member. Read through [`agent_in::AgentSpecIn`], which also takes a Bedrock agent
/// (`{kind: "bedrock", model_id, region}`) and makes it an `open` agent on provider
/// `bedrock:<region>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "agent_in::AgentSpecIn")]
pub struct AgentSpec {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
    pub model: Option<String>,
    pub prompt: String,
    /// Where an open agent's model calls go: `openrouter`, `openai`, `anthropic` or
    /// `local:<name>`. Absent (or blank) means the default provider, `openrouter`, as in runs and
    /// teams made before providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// True when the team keeps this agent from writing: it gets no write tools, and a write call
    /// is refused. Absent means false, so every agent may use every tool unless its team says
    /// otherwise. Always stored: a stored team without it was recorded under the old rule (only
    /// the last agent writes), which [`legacy_read_only`] applies.
    #[serde(default)]
    pub read_only: bool,
    /// True for the agent that leads a `dispatch` run (absent: the team's first agent leads).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lead: bool,
    /// Where an `http` agent answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The headers an `http` agent's requests carry; their values are secrets, never written
    /// out (see [`crate::secrets`]).
    #[serde(default, skip_serializing_if = "Headers::is_empty")]
    pub headers: Headers,
    /// The world tools this agent may see and call, by name (empty: every tool). A restriction
    /// of the setup, like `read_only`; the talk tools of its runner are always offered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

impl AgentSpec {
    /// Agent `id` of `kind`, named `id`, with what a stored agent that says nothing more has: no
    /// model, prompt or provider (the defaults), free to write and to use every tool, not the lead.
    pub fn new(id: impl Into<String>, kind: AgentKind) -> AgentSpec {
        let id = id.into();
        AgentSpec {
            name: id.clone(),
            id,
            kind,
            model: None,
            prompt: String::new(),
            provider: None,
            read_only: false,
            lead: false,
            url: None,
            headers: Headers::default(),
            tools: Vec::new(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> AgentSpec {
        self.name = name.into();
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> AgentSpec {
        self.model = Some(model.into());
        self
    }

    pub fn with_prompt(mut self, prompt: impl Into<String>) -> AgentSpec {
        self.prompt = prompt.into();
        self
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> AgentSpec {
        self.provider = Some(provider.into());
        self
    }

    /// True when the agent's setup lets it see and call the world tool `name`.
    pub fn may_use(&self, name: &str) -> bool {
        self.tools.is_empty() || self.tools.iter().any(|t| t == name)
    }
}

/// The agents of a run. How they run (in order, at once) is the run's runner, not the team's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub agents: Vec<AgentSpec>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub kind: Option<AgentKind>,
    /// A new provider (empty or blank text: back to `openrouter`); a model change for the fork
    /// rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Whether the agent is kept from writing (see [`AgentSpec::read_only`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    /// The world tools the agent may see and call after the fork: exactly this list (see
    /// [`AgentSpec::tools`]; empty: every tool again).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
}

/// A stored team as it ran: an agent recorded before `read_only` existed (no such key) ran under
/// the old rule, where only the last agent could write, so it is read-only unless it is last.
pub fn legacy_read_only(team: &mut Value) {
    let Some(agents) = team.get_mut("agents").and_then(Value::as_array_mut) else {
        return;
    };
    let last = agents.len().saturating_sub(1);
    for (i, a) in agents.iter_mut().enumerate() {
        if let Some(a) = a.as_object_mut() {
            a.entry("read_only").or_insert(Value::Bool(i != last));
        }
    }
}

/// Who drives a run's agents.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Runner {
    /// shaked runs the agents one after another, in the team's order.
    #[default]
    Chain,
    /// shaked runs every agent at once.
    Concurrent,
    /// shaked starts the lead with the ticket; the lead delegates work to the other agents, who
    /// wake when work or a message arrives, report back and stay available, until the lead
    /// finishes the run (see `crate::dispatch`).
    Dispatch,
    /// An external system registers the agents and drives them through their endpoints; shaked
    /// records what they do and scores the world when the system says the run is finished.
    External,
}

impl Team {
    /// The index of the agent that leads a `dispatch` run: the one marked `lead`, else the first.
    pub fn lead_index(&self) -> usize {
        self.agents.iter().position(|a| a.lead).unwrap_or(0)
    }
}

/// The rules a branch runs under, recorded in its run's `RunStart` (`runner`, `handoff`,
/// `max_cost_usd`, `connection`, `world_minutes`).
#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    pub runner: Runner,
    /// True for runs recorded before handoffs were optional: every agent but the last must hand
    /// off, the last may not, and an agent's input has only the previous agent's note.
    pub handoff_required: bool,
    /// The run's model-spend limit in USD (`crate::budget`); None: no limit.
    pub max_cost_usd: Option<f64>,
    /// The Code connection whose driver Shakedown launched for the run (`crate::driver`).
    pub connection: Option<String>,
    /// The run's world time window in minutes (`crate::window`): a run an external system
    /// drives is scored when it ends; None: no window.
    pub world_minutes: Option<f64>,
}

impl Rules {
    /// What a new run gets with `runner`: handoffs and messages optional.
    pub fn new(runner: Runner) -> Rules {
        Rules {
            runner,
            handoff_required: false,
            max_cost_usd: crate::budget::env_default(),
            connection: None,
            world_minutes: None,
        }
    }

    /// These rules with the world time window `m` (minutes; see `crate::window::valid_minutes`).
    pub fn with_world_minutes(mut self, m: Option<f64>) -> Rules {
        self.world_minutes = m;
        self
    }

    /// These rules with the spend limit `max` (`None` keeps the env default).
    pub fn with_max_cost(mut self, max: Option<f64>) -> Rules {
        if let Some(m) = max.and_then(crate::budget::positive) {
            self.max_cost_usd = Some(m);
        }
        self
    }

    /// The rules of a run recorded before runners: a chain with a handoff required.
    pub fn legacy() -> Rules {
        Rules {
            runner: Runner::Chain,
            handoff_required: true,
            max_cost_usd: None,
            connection: None,
            world_minutes: None,
        }
    }

    /// The rules a `RunStart` records; one without `runner` is a legacy run.
    pub fn of(start: &RunStartPayload) -> Rules {
        match start.runner {
            Some(runner) => Rules {
                runner,
                handoff_required: start.handoff.as_deref() == Some("required"),
                max_cost_usd: start.max_cost_usd.and_then(crate::budget::positive),
                connection: start.connection.clone(),
                world_minutes: start.world_minutes,
            },
            None => Rules::legacy(),
        }
    }

    /// These rules with the Code connection `id` (`None` keeps none).
    pub fn with_connection(mut self, id: Option<String>) -> Rules {
        self.connection = id;
        self
    }

    /// How `RunStart` writes `handoff_required` (its field `handoff`).
    pub fn handoff_word(&self) -> &'static str {
        match self.handoff_required {
            true => "required",
            false => "optional",
        }
    }
}

named_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BranchStatus {
        Running = "running",
        Passed = "passed",
        Failed = "failed",
        InfraError = "infra_error",
        Interrupted = "interrupted",
        /// Someone stopped the run before it ended; it is not scored.
        Stopped = "stopped",
        /// The run's model-spend estimate passed its `max_cost_usd`: its agents were stopped and
        /// the world scored as it stood (a failure).
        BudgetExceeded = "budget_exceeded",
    }
}

/// `text` trimmed, `None` when that leaves nothing: what a blank provider or model means.
pub fn trimmed(text: &str) -> Option<String> {
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// A running cost total: unknown (`None`) as soon as any part of it is unknown.
pub fn add_cost(total: Option<f64>, more: Option<f64>) -> Option<f64> {
    Some(total? + more?)
}

named_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum EventKind {
        RunStart = "run_start",
        AgentStart = "agent_start",
        ModelCall = "model_call",
        ToolCall = "tool_call",
        Handoff = "handoff",
        Message = "message",
        AgentEnd = "agent_end",
        Check = "check",
        WorldEvent = "world_event",
        /// The world's view after a step (`{seq_of, view}`), when the benchmark draws one.
        WorldView = "world_view",
        /// Someone paused the branch: its calls wait and its clock stands still until `Resume`.
        Pause = "pause",
        /// The branch goes on after a `Pause` (`{paused_ms}`).
        Resume = "resume",
        /// A dispatch run's lead gives an agent work (`{to, task}`).
        Delegation = "delegation",
        /// An agent of a dispatch run reports to its lead (`{to, text, turn_end?}`).
        Report = "report",
        /// shaked starts an agent's turn in a dispatch run (`{reason, mail?, input}`).
        Wake = "wake",
        /// An agent of a dispatch run ended its turn and waits (`{ok, error?, text?}`).
        Idle = "idle",
        /// The lead of a dispatch run ends it (`{summary}`).
        Finish = "finish",
        /// What an agent thought or said it would do (`{text, kind, source, model_call_seq?,
        /// truncated?}`), reported by its driver or pulled from a recorded model response.
        Reasoning = "reasoning",
        /// shaked saved the parts that save at points (`{reason, saves, error?, paused_ms}`):
        /// bookkeeping, no step.
        SavePoint = "save_point",
    }
}

impl EventKind {
    /// True for shaked's own records that are no step of the run: the world's views and save
    /// points.
    pub fn is_bookkeeping(self) -> bool {
        matches!(self, EventKind::WorldView | EventKind::SavePoint)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub branch_id: String,
    pub seq: i64,
    pub at: DateTime<Utc>,
    pub agent: Option<String>,
    pub kind: EventKind,
    pub payload: Value,
}

/// Payload of `EventKind::ToolCall`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldEventPayload {
    pub part: String,
    pub kind: String,
    pub text: String,
    pub data: Value,
    pub clock_ms: i64,
}

/// Payload of `EventKind::ModelCall`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

/// Tokens of model calls, OpenAI-shaped: what an agent's calls add up to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

impl TokenUsage {
    /// `prompt` and `completion` tokens, and their sum as the total.
    pub fn summed(prompt: i64, completion: i64) -> TokenUsage {
        TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
        }
    }
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandoffPayload {
    pub note: String,
    /// The agent the note is for; absent: for whoever comes next.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

/// Payload of `EventKind::Message`: one agent's message to another, or to all (`to` = `*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessagePayload {
    pub to: String,
    pub text: String,
}

/// Payload of `EventKind::Delegation`: the lead gives agent `to` its `task`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl EventPayload for AgentEndPayload {
    const KIND: EventKind = EventKind::AgentEnd;
}

impl EventPayload for ReportPayload {
    const KIND: EventKind = EventKind::Report;
}

/// Payload of `EventKind::Finish`: the lead's summary of the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinishPayload {
    pub summary: String,
}

/// `to` of a message for every agent.
pub const EVERYONE: &str = "*";

/// Payload of `EventKind::Check`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// The restore a fork asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RestoreMode {
    /// The world as it was just before the fork point, exactly.
    Exact,
    /// The world as it was just before the fork point, approximately where a part can't be
    /// exact.
    Approximate,
    /// For evaluation only (the snapshots-off ablation): the agents' context up to the fork
    /// point, but the world as the parent ended, as forks that keep no world state do. Never a
    /// default: only a request that names it gets it.
    #[serde(rename = "none")]
    NotRestored,
}

impl RestoreMode {
    /// The mode each part's restore is planned in: a fork that leaves the world as it ended
    /// starts each part from its end save, exact or approximate as the part keeps it.
    pub fn parts_mode(self) -> Mode {
        match self {
            RestoreMode::Exact => Mode::Exact,
            RestoreMode::Approximate | RestoreMode::NotRestored => Mode::Approximate,
        }
    }
}

/// How a part of a rerun's world was restored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RestoredHow {
    /// From its save, exactly.
    Save,
    /// By repeating its recorded inputs.
    Replay,
    /// From its save, approximately.
    Approximate,
    /// Not put back: left as the parent branch ended (a fork with [`RestoreMode::NotRestored`]).
    #[serde(rename = "none")]
    NotRestored,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestoredPart {
    pub part: String,
    pub how: RestoredHow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    pub id: String,
    pub run_id: String,
    pub parent_id: Option<String>,
    pub start_seq: Option<i64>,
    pub team: Team,
    pub changes: Value,
    pub fork_note: Option<String>,
    pub status: BranchStatus,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    /// The restore a fork asked for (None for a run's first branch).
    pub restore_mode: Option<RestoreMode>,
    /// How each part of a fork's world was restored (empty for a run's first branch).
    pub restored: Vec<RestoredPart>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub task_id: String,
    pub created_at: DateTime<Utc>,
    pub branches: Vec<Branch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind's and status's name, as stored and as the API shows it.
    const KINDS: [&str; 19] = [
        "run_start",
        "agent_start",
        "model_call",
        "tool_call",
        "handoff",
        "message",
        "agent_end",
        "check",
        "world_event",
        "world_view",
        "pause",
        "resume",
        "delegation",
        "report",
        "wake",
        "idle",
        "finish",
        "reasoning",
        "save_point",
    ];
    const STATUSES: [&str; 7] = [
        "running",
        "passed",
        "failed",
        "infra_error",
        "interrupted",
        "stopped",
        "budget_exceeded",
    ];

    #[test]
    fn each_event_kind_has_one_name_both_ways() {
        let names: Vec<&str> = EventKind::ALL.iter().map(EventKind::as_str).collect();
        assert_eq!(names, KINDS);
        for k in EventKind::ALL {
            assert_eq!(EventKind::parse(k.as_str()), Some(*k));
            assert_eq!(serde_json::to_value(k).unwrap(), k.as_str());
            let read: EventKind = serde_json::from_value(k.as_str().into()).unwrap();
            assert_eq!(read, *k);
        }
        assert_eq!(EventKind::parse("nope"), None);
    }

    #[test]
    fn each_branch_status_has_one_name_both_ways() {
        let names: Vec<&str> = BranchStatus::ALL.iter().map(BranchStatus::as_str).collect();
        assert_eq!(names, STATUSES);
        for s in BranchStatus::ALL {
            assert_eq!(BranchStatus::parse(s.as_str()), Some(*s));
            assert_eq!(serde_json::to_value(s).unwrap(), s.as_str());
            let read: BranchStatus = serde_json::from_value(s.as_str().into()).unwrap();
            assert_eq!(read, *s);
        }
        assert_eq!(BranchStatus::parse("nope"), None);
    }
}
