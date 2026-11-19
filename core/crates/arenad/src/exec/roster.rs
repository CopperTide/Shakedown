//! Who is on a running branch: its team, the agents that joined it, and the bearer tokens their
//! endpoints want.

use std::{
    collections::HashMap,
    sync::{Mutex, RwLock},
};

use crate::model::{AgentSpec, Team};

/// A running branch's agents.
pub struct Roster {
    pub team: Team,
    /// Agents an external system registered while the branch runs (not in `team`).
    joined: RwLock<Vec<AgentSpec>>,
    /// agent id -> the bearer token its MCP endpoint wants, for an agent given one (an `http`
    /// agent, while arenad waits for it). Kept in memory only.
    tokens: Mutex<HashMap<String, String>>,
    /// The agent whose model call through the gateway came last (a driver's own HTTP requests
    /// are attributed to it).
    last_model_caller: Mutex<Option<String>>,
}

impl Roster {
    pub fn new(team: Team) -> Roster {
        Roster {
            team,
            joined: Default::default(),
            tokens: Default::default(),
            last_model_caller: Default::default(),
        }
    }

    /// The id of the agent that leads a dispatch run (see [`Team::lead_index`]).
    pub fn lead_id(&self) -> String {
        let lead = self.team.agents.get(self.team.lead_index());
        lead.map(|a| a.id.clone()).unwrap_or_default()
    }

    pub fn is_lead(&self, agent: &str) -> bool {
        self.lead_id() == agent
    }

    pub fn is_last(&self, agent: &str) -> bool {
        self.team.agents.last().is_some_and(|a| a.id == agent)
    }

    /// The agent `id`: one of the team, or one that joined while the branch runs.
    pub fn agent(&self, id: &str) -> Option<AgentSpec> {
        let known = self.team.agents.iter().find(|a| a.id == id).cloned();
        known.or_else(|| {
            self.joined
                .read()
                .unwrap()
                .iter()
                .find(|a| a.id == id)
                .cloned()
        })
    }

    /// Every agent: the team's, then those that joined, in order.
    pub fn agents(&self) -> Vec<AgentSpec> {
        let mut all = self.team.agents.clone();
        all.extend(self.joined.read().unwrap().iter().cloned());
        all
    }

    /// Adds an agent that joined while the branch runs; false when one has its id already.
    pub fn join(&self, spec: AgentSpec) -> bool {
        let mut joined = self.joined.write().unwrap();
        let taken = self
            .team
            .agents
            .iter()
            .chain(joined.iter())
            .any(|a| a.id == spec.id);
        if !taken {
            joined.push(spec);
        }
        !taken
    }

    /// True when `agent`'s team keeps it from writing (an unknown agent writes nothing either).
    pub fn read_only(&self, agent: &str) -> bool {
        self.agent(agent).is_none_or(|a| a.read_only)
    }

    /// True when `agent`'s setup lets it use the world tool `name` (an unknown agent uses none).
    pub fn may_use(&self, agent: &str, name: &str) -> bool {
        self.agent(agent).is_some_and(|a| a.may_use(name))
    }

    pub fn has_agent(&self, agent: &str) -> bool {
        self.agent(agent).is_some()
    }

    /// The provider `agent`'s model calls go to (`Some(None)`: it names none, so the default
    /// one); `None` when the branch has no such agent.
    pub fn provider_of(&self, agent: &str) -> Option<Option<String>> {
        self.agent(agent).map(|a| a.provider)
    }

    /// Notes that `agent` makes a model call through the gateway.
    pub fn note_model_call(&self, agent: &str) {
        *self.last_model_caller.lock().unwrap() = Some(agent.to_string());
    }

    /// The agent whose model call came last, if any did.
    pub fn last_model_caller(&self) -> Option<String> {
        self.last_model_caller.lock().unwrap().clone()
    }

    /// A new bearer token for `agent`'s MCP endpoint, which takes no call without it from now on.
    pub fn issue_token(&self, agent: &str) -> String {
        let token = uuid::Uuid::new_v4().simple().to_string();
        self.tokens
            .lock()
            .unwrap()
            .insert(agent.into(), token.clone());
        token
    }

    /// Takes back `agent`'s token: its endpoint answers no one until another is issued.
    pub fn revoke_token(&self, agent: &str) {
        self.tokens
            .lock()
            .unwrap()
            .insert(agent.into(), String::new());
    }

    /// True when a call to `agent`'s endpoint with `authorization` (the header's value) may go
    /// through: the agent was never given a token, or this is it.
    pub fn token_ok(&self, agent: &str, authorization: Option<&str>) -> bool {
        match self.tokens.lock().unwrap().get(agent) {
            None => true,
            Some(t) => {
                let given = authorization.and_then(|a| a.strip_prefix("Bearer "));
                !t.is_empty() && given == Some(t.as_str())
            }
        }
    }
}
