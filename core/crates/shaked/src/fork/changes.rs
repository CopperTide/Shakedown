//! The agent changes a fork applies: which ones really change an agent, settling provider names,
//! applying them to a team, and the tools they add beyond an agent's setup.

use std::collections::HashMap;

use crate::{model::*, providers::ProviderConfig};

/// True when `c` changes the agent's mode (a kind equal to the current one is no change).
pub(super) fn changes_kind(c: &Change, spec: &AgentSpec) -> bool {
    c.kind.is_some_and(|k| k != spec.kind)
}

/// A text a change sets: `Some(None)` clears it (empty or blank text), `Some(Some(t))` sets it.
fn new_text(text: Option<&String>) -> Option<Option<String>> {
    text.map(|t| trimmed(t))
}

/// The model a change sets (see `new_text`).
fn new_model(c: &Change) -> Option<Option<String>> {
    new_text(c.model.as_ref())
}

/// True when `c` changes the agent's model (the same model, or clearing an unset one, is no change).
fn changes_model(c: &Change, spec: &AgentSpec) -> bool {
    new_model(c).is_some_and(|m| m != spec.model)
}

/// The provider a change sets (see `new_text`; cleared is back to the default one).
fn new_provider(c: &Change) -> Option<Option<String>> {
    new_text(c.provider.as_ref())
}

/// True when `c` names another provider than the agent's. Changes are settled first
/// ([`settle_changes`]), so a name for the agent's own provider is no longer there.
fn changes_provider(c: &Change, spec: &AgentSpec) -> bool {
    new_provider(c).is_some_and(|p| p != spec.provider)
}

/// Drops each provider change that names the agent's own provider (as `providers` resolves
/// both: absent is the default one), so the fork rules see only real ones.
pub fn settle_changes(
    team: &Team,
    changes: &mut HashMap<String, Change>,
    providers: &ProviderConfig,
) {
    for spec in &team.agents {
        let Some(c) = changes.get_mut(&spec.id) else {
            continue;
        };
        let same = new_provider(c).is_some_and(|p| {
            providers.id_of(p.as_deref()) == providers.id_of(spec.provider.as_deref())
        });
        if same {
            c.provider = None;
        }
    }
}

/// True when `c` makes the agent read-only or lets it write again.
fn changes_read_only(c: &Change, spec: &AgentSpec) -> bool {
    c.read_only.is_some_and(|r| r != spec.read_only)
}

/// True when `c` gives the agent another list of world tools.
fn changes_tools(c: &Change, spec: &AgentSpec) -> bool {
    c.tools.as_ref().is_some_and(|t| *t != spec.tools)
}

/// A provider change is a model change: the fork rules treat both alike. A read-only or tools
/// change is treated like a prompt change.
pub(super) fn is_change(c: &Change, spec: &AgentSpec) -> bool {
    c.prompt.is_some()
        || changes_read_only(c, spec)
        || changes_tools(c, spec)
        || changes_model(c, spec)
        || changes_provider(c, spec)
        || changes_kind(c, spec)
}

/// For each agent whose setup lists its tools, the tools `changes` give it beyond that list (by
/// agent id, sorted).
pub fn tools_added(team: &Team, changes: &HashMap<String, Change>) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = team
        .agents
        .iter()
        .filter(|spec| !spec.tools.is_empty())
        .filter_map(|spec| {
            let listed = changes.get(&spec.id)?.tools.as_ref()?;
            let added: Vec<String> = (listed.iter())
                .filter(|t| !spec.tools.contains(t))
                .cloned()
                .collect();
            (!added.is_empty()).then(|| (spec.id.clone(), added))
        })
        .collect();
    out.sort();
    out
}

/// `team` with each agent's prompt/model/kind/provider/read_only/tools replaced where `changes`
/// sets them.
pub fn apply_changes(team: &Team, changes: &HashMap<String, Change>) -> Team {
    let mut t = team.clone();
    for spec in &mut t.agents {
        if let Some(c) = changes.get(&spec.id) {
            apply_change(spec, c);
        }
    }
    t
}

fn apply_change(spec: &mut AgentSpec, c: &Change) {
    if let Some(p) = &c.prompt {
        spec.prompt = p.clone();
    }
    if changes_kind(c, spec) || changes_provider(c, spec) {
        // The old model belongs to the old driver or provider: the new one's default applies.
        spec.model = None;
    }
    if let Some(m) = new_model(c) {
        spec.model = m;
    }
    if let Some(p) = new_provider(c) {
        spec.provider = p;
    }
    if let Some(k) = c.kind {
        spec.kind = k;
    }
    if let Some(r) = c.read_only {
        spec.read_only = r;
    }
    if let Some(t) = &c.tools {
        spec.tools = t.clone();
    }
}
