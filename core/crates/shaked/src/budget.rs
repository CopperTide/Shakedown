//! A run's model-spend limit (`max_cost_usd`), enforced while agents work. Each model call an
//! agent makes is priced as it is seen (Claude Code's stream-json `usage`, an open agent's call
//! through the gateway) at the list prices of `docs/prices.toml`, and added to the run's running
//! estimate; a turn's reported cost, when it comes, replaces that agent's estimate. Once the
//! estimate passes the limit the agent's turn is stopped and the run ends `budget_exceeded`,
//! scored as it stands (a failure). An agent kind whose usage is seen only at a turn's end (the
//! one-shot `claude -p`, Codex) is checked when that turn ends.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::prices::{self, CallTokens};

/// Env default for a run that names no `max_cost_usd`.
pub const ENV: &str = "SHAKEDOWN_MAX_COST_USD";

/// The limit `SHAKEDOWN_MAX_COST_USD` sets for runs that name none: a positive number, else none.
pub fn env_default() -> Option<f64> {
    positive(parts::settings::var(ENV).ok()?.trim().parse().ok()?)
}

/// `v` when it is a positive finite number of dollars.
pub fn positive(v: f64) -> Option<f64> {
    (v.is_finite() && v > 0.0).then_some(v)
}

/// The reason a run ends `budget_exceeded`.
pub fn reason(spent: f64, limit: f64) -> String {
    format!("the run's model spend estimate, USD {spent:.2}, passed its limit of USD {limit:.2}")
}

/// One run's limit and its running estimate, per agent.
#[derive(Debug, Default)]
pub struct Budget {
    limit: Option<f64>,
    /// agent -> (cost reported by finished turns, estimate of calls since then)
    spent: Mutex<HashMap<String, (f64, f64)>>,
    /// Set once a model call was refused for the limit.
    refused: AtomicBool,
}

impl Budget {
    pub fn new(limit: Option<f64>) -> Budget {
        Budget {
            limit: limit.and_then(positive),
            spent: Mutex::default(),
            refused: AtomicBool::new(false),
        }
    }

    pub fn limit(&self) -> Option<f64> {
        self.limit
    }

    /// The run's estimate so far: every agent's reported cost plus its calls since.
    pub fn spent(&self) -> f64 {
        let s = self.spent.lock().unwrap();
        s.values().map(|(r, live)| r + live).sum()
    }

    /// `agent`'s calls since its last reported cost.
    pub fn live(&self, agent: &str) -> f64 {
        self.spent.lock().unwrap().get(agent).map_or(0.0, |v| v.1)
    }

    /// Adds one call of `agent`, priced live; the reason to stop when the run is now over.
    pub fn add(&self, agent: &str, usd: f64) -> Option<String> {
        self.spent
            .lock()
            .unwrap()
            .entry(agent.to_string())
            .or_default()
            .1 += usd.max(0.0);
        self.over()
    }

    /// `agent`'s reported cost so far (cumulative), which replaces its live estimate; the reason
    /// to stop when the run is now over.
    pub fn settle(&self, agent: &str, reported_total: f64) -> Option<String> {
        self.spent
            .lock()
            .unwrap()
            .insert(agent.to_string(), (reported_total.max(0.0), 0.0));
        self.over()
    }

    /// True the first time it is asked (the first model call refused for the limit), false after.
    pub fn first_refusal(&self) -> bool {
        !self.refused.swap(true, Ordering::SeqCst)
    }

    /// The reason to stop: the estimate is past the limit.
    pub fn over(&self) -> Option<String> {
        let limit = self.limit?;
        let spent = self.spent();
        (spent > limit).then(|| reason(spent, limit))
    }
}

/// What one call of `model` costs at list price on `provider`, when the table has the model.
pub fn price(provider: &str, model: Option<&str>, t: &CallTokens) -> Option<f64> {
    prices::call_cost(provider, model?, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_is_over_once_its_estimate_passes_the_limit() {
        let b = Budget::new(Some(1.0));
        assert_eq!(b.add("lead", 0.6), None);
        assert_eq!(b.add("supply", 0.3), None);
        let why = b.add("lead", 0.2).unwrap();
        assert!(
            why.contains("USD 1.10") && why.contains("USD 1.00"),
            "{why}"
        );
        assert!((b.live("lead") - 0.8).abs() < 1e-9);
    }

    #[test]
    fn a_reported_cost_replaces_the_agents_estimate() {
        let b = Budget::new(Some(1.0));
        b.add("lead", 0.9);
        assert_eq!(
            b.settle("lead", 0.5),
            None,
            "the turn's real cost was lower"
        );
        assert_eq!((b.spent(), b.live("lead")), (0.5, 0.0));
        assert!(b.settle("lead", 1.2).is_some());
    }

    #[test]
    fn no_limit_never_stops_and_bad_limits_are_none() {
        let b = Budget::new(None);
        assert_eq!(b.add("a", 1e6), None);
        assert_eq!(Budget::new(Some(0.0)).limit(), None);
        assert_eq!(Budget::new(Some(f64::NAN)).limit(), None);
        assert_eq!(positive(-1.0), None);
    }

    #[test]
    fn a_haiku_call_is_priced_from_the_table() {
        let t = CallTokens {
            cache_read: 1_000_000,
            ..CallTokens::default()
        };
        assert!(price("anthropic", Some("claude-haiku-5-5"), &t).unwrap() > 0.0);
        assert_eq!(price("anthropic", None, &t), None);
    }
}
