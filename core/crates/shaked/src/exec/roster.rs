//! Who is on a running branch: its team, the agents that joined it, and the bearer tokens their
//! endpoints want.

use std::sync::{Mutex, RwLock};

use crate::model::{AgentSpec, Team};

/// A running branch's agents.
pub struct Roster {
    pub team: Team,
    /// Agents an external system registered while the branch runs (not in `team`).
    joined: RwLock<Vec<AgentSpec>>,
    /// The agents' tokens (`sda_…`), which their MCP endpoint and model gateway want.
    tokens: super::agent_tokens::AgentTokens,
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

    /// A new token for `agent` (`sda_…`), valid for `ttl` of unpaused time.
    pub fn issue_token(&self, agent: &str, ttl: std::time::Duration) -> String {
        self.tokens.issue(agent, ttl)
    }

    /// `agent`'s good token, else a new one valid for `ttl`.
    pub fn ensure_token(&self, agent: &str, ttl: std::time::Duration) -> String {
        self.tokens.ensure(agent, ttl)
    }

    /// `agent`'s token while it is good: in memory only, for handing to the agent.
    pub fn token_of(&self, agent: &str) -> Option<String> {
        self.tokens.token_of(agent)
    }

    /// Takes back `agent`'s token: its endpoints answer no one until another is issued.
    pub fn revoke_token(&self, agent: &str) {
        self.tokens.revoke(agent);
    }

    /// Takes back every agent's token (the branch ended).
    pub fn revoke_all(&self) {
        self.tokens.revoke_all();
    }

    /// Moves every token's expiry `by` later (the branch was paused that long).
    pub fn extend(&self, by: std::time::Duration) {
        self.tokens.extend(by);
    }

    /// True when a call to `agent`'s endpoints with `authorization` (the header's value) may go
    /// through: it carries the agent's good token. An agent never given one gets nothing.
    pub fn token_ok(&self, agent: &str, authorization: Option<&str>) -> bool {
        self.tokens.ok(agent, authorization)
    }
}
