//! What one ended branch came to, worked out once from its record and kept in `branches.stats`,
//! so the dashboards aggregate small rows instead of every event. The figures are the ones the run
//! page shows ([`metrics`](crate::metrics::metrics), over the same price table).

use std::collections::BTreeMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    metrics::{metrics, AgentUse},
    model::{Branch, Event, RunStartPayload},
    prices,
};

/// The version of [`BranchStats`]' shape: a row of another version is worked out again. Version 3
/// also keeps each model call's usage (`call_usage`), so every ended branch gets it once.
pub const VERSION: u32 = 3;

/// What an ended branch came to (`branches.stats`, plus the `ended_at` it was worked out for).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchStats {
    pub v: u32,
    /// The run's world (its `RunStart`'s benchmark; empty when it named none).
    pub world: String,
    /// The run's scenario (its task).
    pub scenario: String,
    /// The agent setup's name.
    pub setup: String,
    /// The model that used the most tokens, else the setup's first agent's ("unknown": neither).
    pub model: String,
    /// Wall-clock seconds from its start to its end.
    pub seconds: f64,
    pub tokens: i64,
    /// None when some call's cost is not known.
    pub cost_usd: Option<f64>,
    /// Explicit failed names from the last scoring event, once per run.
    pub failed_checks: Vec<String>,
    /// Tool names with a recorded refusal, once per run (not individual gate ids).
    pub refused_tools: Vec<String>,
}

/// The stats of `branch`, a branch of a run of `task`, from its `events` (model-call bodies may be
/// left out: usage and cost are what count).
pub fn stats_of(branch: &Branch, task: &str, events: &[Event]) -> BranchStats {
    let m = metrics(branch, events, prices::table(), Utc::now());
    let start = RunStartPayload::of(events).unwrap_or_default();
    BranchStats {
        v: VERSION,
        world: start.benchmark.unwrap_or_default(),
        scenario: task.to_string(),
        setup: branch.team.name.clone(),
        model: top_model(&m.agents, branch),
        seconds: m.measured_s,
        tokens: m.tokens,
        cost_usd: m.cost.usd,
        failed_checks: super::failures::failed_checks(events),
        refused_tools: super::failures::refused_tools(events),
    }
}

/// The model whose agents used the most tokens (ties: the first by name), else the setup's first
/// agent's model.
fn top_model(agents: &[AgentUse], branch: &Branch) -> String {
    let mut by: BTreeMap<&str, i64> = BTreeMap::new();
    for a in agents.iter().filter(|a| a.tokens > 0) {
        *by.entry(a.model.as_deref().unwrap_or("unknown"))
            .or_default() += a.tokens;
    }
    let top = by
        .into_iter()
        .fold(None::<(&str, i64)>, |best, (m, t)| match best {
            Some((_, bt)) if bt >= t => best,
            _ => Some((m, t)),
        });
    let planned = || branch.team.agents.iter().find_map(|a| a.model.clone());
    top.map(|(m, _)| m.to_string())
        .or_else(planned)
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{AgentKind, AgentSpec, BranchStatus, EventKind},
        testkit,
    };
    use serde_json::json;

    fn ended(models: &[&str]) -> Branch {
        let mut b = testkit::branch(BranchStatus::Passed);
        b.ended_at = Some(b.created_at + chrono::Duration::seconds(4));
        b.team.name = "Solo".into();
        b.team.agents = models
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let mut a = AgentSpec::new(format!("a{i}"), AgentKind::Open);
                a.model = Some((*m).into());
                a
            })
            .collect();
        b
    }

    fn call(seq: i64, agent: &str, model: &str, tokens: i64, cost: f64) -> Event {
        let usage =
            json!({"prompt_tokens": tokens, "completion_tokens": 0, "total_tokens": tokens});
        testkit::event(
            seq,
            Some(agent),
            EventKind::ModelCall,
            json!({"request": {"model": model}, "response": null, "usage": usage, "cost_usd": cost}),
        )
    }

    fn start(seq: i64, agent: &str, model: &str) -> Event {
        testkit::event(
            seq,
            Some(agent),
            EventKind::AgentStart,
            json!({"name": agent, "kind": "open", "model": model}),
        )
    }

    #[test]
    fn a_run_comes_to_its_world_setup_top_model_time_tokens_and_cost() {
        let b = ended(&["small", "big"]);
        let events = [
            testkit::event(0, None, EventKind::RunStart, json!({"benchmark": "retail"})),
            start(1, "a0", "small"),
            start(2, "a1", "big"),
            call(3, "a0", "small", 100, 0.01),
            call(4, "a1", "big", 900, 0.09),
        ];
        let s = stats_of(&b, "refund", &events);
        assert_eq!(
            (s.world.as_str(), s.scenario.as_str()),
            ("retail", "refund")
        );
        assert_eq!((s.setup.as_str(), s.model.as_str()), ("Solo", "big"));
        assert_eq!(s.tokens, 1000);
        assert!((s.cost_usd.unwrap() - 0.10).abs() < 1e-9);
        assert!((s.seconds - 4.0).abs() < 0.01);
    }

    #[test]
    fn a_run_without_calls_takes_its_setups_model() {
        let s = stats_of(&ended(&["planned"]), "t", &[]);
        assert_eq!(
            (s.model.as_str(), s.tokens, s.world.as_str()),
            ("planned", 0, "")
        );
        assert_eq!(stats_of(&ended(&[]), "t", &[]).model, "unknown");
    }
}
