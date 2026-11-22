//! Whether a team can run: its agents' ids, providers and URLs, and what its runner needs. Each
//! refusal says what is wrong (the API answers it as a 400).

use std::collections::HashSet;

use super::{AgentKind, AgentSpec, Runner, Team};
use crate::providers::ProviderConfig;

/// Why a team can't run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeamError {
    #[error("the team needs at least one agent")]
    NoAgents,
    #[error("an external agent runs only in a run with runner \"external\"")]
    External,
    #[error("mark at most one agent as the lead")]
    TwoLeads,
    #[error("agent {0:?} is an http agent, which answers one run at a time and can't take turns: a dispatch run needs Claude Code or shaked's own loop (open)")]
    HttpInDispatch(String),
    #[error("agent {0:?} is a Codex agent, which can't keep a session across turns yet: a dispatch run needs Claude Code or shaked's own loop (open)")]
    CodexInDispatch(String),
    #[error("agent id {0:?} must match ^[a-z0-9_-]+$")]
    BadId(String),
    #[error("agent id {0:?} is kept for steps the world makes (its links, a task's setup)")]
    KeptId(String),
    #[error("duplicate agent id {0:?}")]
    Duplicate(String),
    /// An open agent's provider can't run its model (the provider's own words).
    #[error("{0}")]
    Provider(String),
    /// An `http` agent's URL or headers are wrong.
    #[error("agent {id:?}: {why}")]
    Http { id: String, why: String },
}

/// Ok unless the team has no agent, or one of its agents is wrong ([`validate_agents`]).
pub fn validate(providers: &ProviderConfig, team: &Team) -> Result<(), TeamError> {
    if team.agents.is_empty() {
        return Err(TeamError::NoAgents);
    }
    validate_agents(providers, &team.agents)
}

/// Ok when shaked can run the team itself under `runner`: some agents, each valid, none
/// external; in a dispatch run at most one lead, and only agents that keep a session.
pub fn validate_builtin(
    providers: &ProviderConfig,
    team: &Team,
    runner: Runner,
) -> Result<(), TeamError> {
    validate(providers, team)?;
    if team.agents.iter().any(|a| a.kind == AgentKind::External) {
        return Err(TeamError::External);
    }
    match runner {
        Runner::Dispatch => validate_dispatch(team),
        _ => Ok(()),
    }
}

/// Refused when two agents are marked `lead`, or an agent can't stay alive across turns.
fn validate_dispatch(team: &Team) -> Result<(), TeamError> {
    if team.agents.iter().filter(|a| a.lead).count() > 1 {
        return Err(TeamError::TwoLeads);
    }
    if let Some(a) = team.agents.iter().find(|a| a.kind == AgentKind::Http) {
        return Err(TeamError::HttpInDispatch(a.id.clone()));
    }
    if let Some(a) = team.agents.iter().find(|a| a.kind == AgentKind::Codex) {
        return Err(TeamError::CodexInDispatch(a.id.clone()));
    }
    Ok(())
}

/// Ok when every agent has a good, unique id, an open agent can run, and an http agent can be
/// reached.
pub fn validate_agents(providers: &ProviderConfig, agents: &[AgentSpec]) -> Result<(), TeamError> {
    let mut seen = HashSet::new();
    for a in agents {
        validate_id(&a.id)?;
        if !seen.insert(a.id.as_str()) {
            return Err(TeamError::Duplicate(a.id.clone()));
        }
        if a.kind != AgentKind::Http && a.auth.is_some() {
            let why = "auth belongs to http agents only".to_string();
            return Err(TeamError::Http {
                id: a.id.clone(),
                why,
            });
        }
        if a.kind == AgentKind::Open {
            open_check(providers, a.provider.as_deref(), a.model.as_deref())?;
        }
        if a.kind == AgentKind::Http {
            validate_http(a)?;
        } else if a.auth.is_some() {
            let why = "auth belongs to http agents only".to_string();
            return Err(TeamError::Http {
                id: a.id.clone(),
                why,
            });
        }
    }
    Ok(())
}

/// Ok when an open agent can run on `provider` with `model`.
pub fn open_check(
    providers: &ProviderConfig,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<(), TeamError> {
    match providers.refusal(provider, model) {
        None => Ok(()),
        Some(why) => Err(TeamError::Provider(why)),
    }
}

/// Ok when an `http` agent has an http(s) URL and every header's value is still held.
pub fn validate_http(a: &AgentSpec) -> Result<(), TeamError> {
    let named = |why: String| TeamError::Http {
        id: a.id.clone(),
        why,
    };
    if let Some(why) = crate::agents::http::bad_url(a.url.as_deref()) {
        return Err(named(why));
    }
    a.headers.reveal().map(|_| ()).map_err(named)
}

/// Ok when `id` matches `^[a-z0-9_-]+$` and isn't kept for the world's own steps.
pub fn validate_id(id: &str) -> Result<(), TeamError> {
    let ok = !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
    if !ok {
        return Err(TeamError::BadId(id.to_string()));
    }
    if id == crate::exec::LINK_ACTOR || id == crate::exec::SETUP_ACTOR {
        return Err(TeamError::KeptId(id.to_string()));
    }
    Ok(())
}
