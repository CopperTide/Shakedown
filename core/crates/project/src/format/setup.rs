//! `setups/<id>.yaml`: an agent team to test, built any way.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Auth, Format};

/// An agent setup: where the team comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetupFile {
    /// Always `shakedown-setup/1`.
    pub format: String,
    /// The setup's display name.
    pub label: String,
    /// The team.
    pub team: Team,
}

impl Format for SetupFile {
    const FORMAT: &'static str = "shakedown-setup/1";
}

/// Where a team comes from. A team described in MADL gets the full graph and checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Team {
    /// Shakedown's built-in agents.
    Builtin {
        /// The agents.
        agents: Vec<AgentSpec>,
    },
    /// Your own code, started as a command.
    Code {
        /// The command, e.g. `uv run python crew.py`.
        command: String,
    },
    /// An agent already deployed behind HTTP.
    Http {
        /// Where tasks are posted.
        url: String,
        /// Where the credential comes from.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth: Option<Auth>,
    },
    /// Claude Code, headless.
    ClaudeCode,
    /// Codex, headless.
    Codex,
    /// A team described in MADL, in a file next to this one.
    Madl {
        /// The MADL file's name, e.g. `team.madl.yaml`.
        file: String,
    },
}

/// How a built-in agent runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AgentRuntime {
    /// API mode: shaked runs the agent loop and calls the model with a key (the default).
    #[default]
    Builtin,
    /// Claude Code, headless, signed in on this machine: no key; `model` is a Claude Code model
    /// (`opus`, `sonnet`, `haiku` or a full id).
    ClaudeCode,
}

impl AgentRuntime {
    fn is_builtin(&self) -> bool {
        *self == AgentRuntime::Builtin
    }
}

/// One built-in agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentSpec {
    /// Unique within the team.
    pub id: String,
    /// How it runs: `builtin` (API mode, the default) or `claude-code`, so a team of several
    /// Claude Code agents (a dispatch team) is one setup.
    #[serde(default, skip_serializing_if = "AgentRuntime::is_builtin")]
    pub kind: AgentRuntime,
    /// The role prompt.
    pub role: String,
    /// The model, e.g. `claude-sonnet`.
    pub model: String,
    /// The key-store name of the model key (none for a `claude-code` agent).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key: String,
    /// Tools it may call; `*` for all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// A `builtin` agent's reasoning, sent with each of its model calls as the request's
    /// `reasoning` (OpenRouter's reasoning tokens): its thinking is then recorded with each turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    /// The most tokens a `builtin` agent's answer may have (reasoning included), sent as each
    /// call's `max_tokens`. Absent, the provider's default (OpenRouter holds credit for the
    /// model's whole output on every call).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// How much a model reasons before it answers (OpenRouter's `reasoning` request field).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reasoning {
    /// `minimal`, `low`, `medium`, `high` or `xhigh`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// A token budget for the reasoning, where the model takes one instead of an effort.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// A reasoning effort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}
