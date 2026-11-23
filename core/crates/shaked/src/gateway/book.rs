//! A call once its provider answered, on every route: what it cost, what it is charged (and
//! added to the run's spend estimate), its one `model_call` event, and the layer's meters of it.

use serde_json::{json, Value};

use super::{
    charge::{self, Charge},
    target::CallFor,
};
use crate::{
    app::AppState,
    layer::{KeySource, Meter, Usage},
    model::*,
    prices::CallTokens,
    providers::Provider,
    reasoning::ReasoningItem,
};

/// How a call to a provider ended, for its bill: it failed (nothing is charged), answered, or
/// its stream was cut before it said what it used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Failed,
    Answered,
    Cut,
}

impl Outcome {
    /// The outcome of an answer with `status`, cut for the reason `interrupted` when given.
    pub(super) fn of(status: u16, interrupted: Option<&str>) -> Outcome {
        match ((200..300).contains(&status), interrupted) {
            (false, _) => Outcome::Failed,
            (true, None) => Outcome::Answered,
            (true, Some(_)) => Outcome::Cut,
        }
    }
}

/// What a call cost: the provider's figure, else its list price for the model it asked for
/// (cached prompt tokens priced as cache hits, cache writes as writes).
fn call_cost(provider: &Provider, p: &ModelCallPayload) -> Option<f64> {
    if p.cost_usd.is_some() {
        return p.cost_usd;
    }
    let n = |v: &Value| v.as_i64().unwrap_or(0);
    let u = &p.usage;
    let cached =
        n(&u["prompt_tokens_details"]["cached_tokens"]).max(n(&u["cache_read_input_tokens"]));
    let written = n(&u["cache_creation_input_tokens"]);
    let t = CallTokens {
        input: (n(&u["prompt_tokens"]) - cached - written).max(0),
        cache_read: cached,
        cache_write: written,
        output: n(&u["completion_tokens"]),
    };
    crate::prices::call_cost(&provider.id, p.request["model"].as_str()?, &t)
}

impl CallFor {
    /// Bills `agent`'s call `p` to `provider` that ended `outcome`: what it is charged (its cost;
    /// on a linked branch a conservative estimate when that can't be known, [`charge::linked`]),
    /// added to the run's spend estimate, and the call's event, which on a linked branch keeps
    /// what was charged (`charged_usd`, `estimated`) for a restarted control box to rebuild the
    /// estimate from.
    pub(super) fn bill(
        &self,
        agent: &str,
        (provider, p): (&Provider, &ModelCallPayload),
        outcome: Outcome,
    ) -> (Charge, Value) {
        let cost = (outcome != Outcome::Failed)
            .then(|| call_cost(provider, p))
            .flatten();
        let cut = outcome == Outcome::Cut;
        let charge = match (self, outcome) {
            (CallFor::Linked(l), Outcome::Answered | Outcome::Cut) => {
                charge::linked(&l.link.budget, (provider, p), cost, cut)
            }
            _ => Charge::of(cost),
        };
        let mut v = serde_json::to_value(p).unwrap_or_default();
        let Some(usd) = charge.usd else {
            return (charge, v);
        };
        match self {
            CallFor::Live(ctx) => drop(ctx.budget.add(agent, usd)),
            CallFor::Linked(l) => {
                drop(l.link.budget.add(agent, usd));
                v["charged_usd"] = json!(usd);
                if charge.estimated {
                    v["estimated"] = json!(true);
                }
            }
        }
        (charge, v)
    }

    /// Records `agent`'s call as its `model_call` event `v`, then its reasoning `items`, then
    /// meters it (what it was charged, its `usage`, whose key paid); the error when it could not
    /// be recorded (nothing else is then).
    pub(super) async fn book(
        &self,
        state: &AppState,
        agent: &str,
        (v, items): (Value, Vec<ReasoningItem>),
        (usd, usage, paid_by): (Option<f64>, &Value, KeySource),
    ) -> Result<(), String> {
        let ev = self.append(state, agent, EventKind::ModelCall, v).await;
        let seq = ev.map_err(|e| format!("{e:#}"))?.seq;
        self.reasoning(state, agent, items, seq).await;
        self.meter(state, usd, usage, (seq, paid_by)).await;
        Ok(())
    }

    /// The layer's meters for one recorded call (its seq, and whose key paid): its cost in
    /// micro-USD and its tokens.
    async fn meter(
        &self,
        state: &AppState,
        cost: Option<f64>,
        usage: &Value,
        (seq, paid_by): (i64, KeySource),
    ) {
        let branch = self.branch_id();
        let key = format!("gateway:{branch}:{seq}");
        let micros = (cost.unwrap_or(0.0) * 1e6).round() as i64;
        let count = |k: &str| usage[k].as_i64().unwrap_or(0);
        let tokens = usage["total_tokens"]
            .as_i64()
            .unwrap_or_else(|| count("prompt_tokens") + count("completion_tokens"));
        let usage = |meter, quantity, key| Usage {
            meter,
            quantity,
            key,
            run: None,
            branch: Some(branch),
            paid_by,
        };
        let (store, caller) = (&state.store, self.caller());
        let cost = usage(Meter::ModelUsdMicros, micros, key.clone());
        crate::layer::meter(store, caller, cost).await;
        let tokens = usage(Meter::ModelTokens, tokens, format!("{key}:tokens"));
        crate::layer::meter(store, caller, tokens).await;
    }
}
