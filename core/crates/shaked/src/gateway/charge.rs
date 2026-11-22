//! What a sandbox's model call is charged to its run's spend estimate on the control box. A cost
//! that can't be known must not let the run spend on unlimited: a call that answered with output
//! but no cost (or a zero one), or whose stream was cut before it said what it used, is charged a
//! conservative estimate instead (marked `estimated`), from the highest prices the table lists for
//! its provider (for an OpenRouter model `vendor/model`, its vendor's); when not even that can be
//! had, a run with a limit is put over it. A run without one has nothing to keep to.

use crate::{
    budget::Budget,
    model::ModelCallPayload,
    providers::{Provider, ProviderKind},
};

/// The output tokens a call that names no limit is assumed to have used.
pub const DEFAULT_OUTPUT_TOKENS: i64 = 4096;

/// What a call is charged: USD, and whether that is the conservative estimate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Charge {
    pub usd: Option<f64>,
    pub estimated: bool,
}

impl Charge {
    /// The call's own `cost`.
    pub fn of(cost: Option<f64>) -> Charge {
        Charge {
            usd: cost,
            estimated: false,
        }
    }
}

/// The output tokens the call can have used, at most: the largest of every output limit the
/// request names (`max_tokens`, `max_output_tokens`, `max_completion_tokens`, each at least 0)
/// and the output it reported; [`DEFAULT_OUTPUT_TOKENS`] when it names none, unless it reported
/// more. A request can't shrink it by naming a small limit the API ignores.
fn output_tokens(p: &ModelCallPayload) -> i64 {
    let names = ["max_tokens", "max_output_tokens", "max_completion_tokens"];
    let limits = names.iter().filter_map(|k| p.request[*k].as_i64());
    let limit = limits
        .map(|n| n.max(0))
        .max()
        .unwrap_or(DEFAULT_OUTPUT_TOKENS);
    let reported = p.usage["completion_tokens"].as_i64().unwrap_or(0);
    limit.max(reported)
}

/// The input tokens of the call: what it reported, else its request's size in bytes over 4
/// (rounded up).
fn input_tokens(p: &ModelCallPayload) -> i64 {
    let reported = p.usage["prompt_tokens"].as_i64().filter(|n| *n > 0);
    let bytes = p.request.to_string().len() as i64;
    reported.unwrap_or((bytes + 3) / 4)
}

/// The highest input and output prices the table lists for `provider`'s calls of `model`: its
/// own, else for OpenRouter those of the model's vendor (`anthropic/…`).
fn highest(provider: &Provider, model: &str) -> Option<(f64, f64)> {
    let table = crate::prices::table();
    let vendor = || match provider.kind {
        ProviderKind::OpenRouter => table.highest(model.split_once('/')?.0),
        _ => None,
    };
    table.highest(&provider.id).or_else(vendor)
}

/// The most the call `p` answered can have cost: its input at the dearest an input token can
/// cost (all of it a cache write), and its output ([`output_tokens`]) at the highest output price.
fn ceiling(provider: &Provider, p: &ModelCallPayload) -> Option<f64> {
    let model = p.request["model"].as_str().unwrap_or_default();
    let (input_price, output_price) = highest(provider, model)?;
    let input = input_tokens(p) as f64 * input_price;
    Some((input + output_tokens(p) as f64 * output_price) / 1e6)
}

/// What a sandbox's successful call `p` to `provider` is charged, its own cost being `cost` and
/// `cut` when its stream stopped short. A local server's calls cost what it declares. A cost
/// that can't be bounded puts a run with a limit over it.
pub fn linked(
    budget: &Budget,
    (provider, p): (&Provider, &ModelCallPayload),
    cost: Option<f64>,
    cut: bool,
) -> Charge {
    let output = p.usage["completion_tokens"].as_i64().unwrap_or(0) > 0;
    let known = cost.is_some_and(|c| c > 0.0 || !output);
    if (known && !cut) || provider.kind == ProviderKind::Local {
        return Charge::of(cost);
    }
    if let Some(usd) = ceiling(provider, p) {
        let usd = Some(usd.max(cost.unwrap_or(0.0)));
        return Charge {
            usd,
            estimated: true,
        };
    }
    if budget.limit().is_some() {
        let id = &provider.id;
        budget.force_over(format!("the cost of a model call on {id} is unknown"));
    }
    Charge::of(cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderConfig;
    use serde_json::{json, Value};

    fn call(usage: Value, request: Value) -> ModelCallPayload {
        ModelCallPayload {
            request,
            response: json!({}),
            usage,
            cost_usd: None,
            cost_source: None,
            provider: None,
            provider_response: None,
        }
    }

    fn claude() -> Provider {
        ProviderConfig::default()
            .resolve(Some("anthropic"))
            .unwrap()
    }

    /// A call of 1000 input and 5 output tokens, its output limit 100.
    fn spoke() -> ModelCallPayload {
        let usage = json!({"prompt_tokens": 1000, "completion_tokens": 5});
        call(usage, json!({"max_tokens": 100}))
    }

    #[test]
    fn an_unknown_cost_is_charged_its_ceiling() {
        let (budget, p) = (Budget::new(Some(1.0)), spoke());
        let known = linked(&budget, (&claude(), &p), Some(0.002), false);
        assert_eq!(known, Charge::of(Some(0.002)));
        let (inp, out) = crate::prices::table().highest("anthropic").unwrap();
        let ceiling = Some((1000.0 * inp + 100.0 * out) / 1e6);
        let zero = linked(&budget, (&claude(), &p), Some(0.0), false);
        assert_eq!((zero.usd, zero.estimated), (ceiling, true));
        let cut = linked(&budget, (&claude(), &p), Some(0.000_001), true);
        assert_eq!((cut.usd, cut.estimated), (ceiling, true));
        let silent = call(
            json!({"prompt_tokens": 10, "completion_tokens": 0}),
            json!({}),
        );
        let free = linked(&budget, (&claude(), &silent), Some(0.0), false);
        assert_eq!(free, Charge::of(Some(0.0)));
        assert!(budget.forced().is_none());
    }

    #[test]
    fn the_ceiling_cannot_be_shrunk_and_counts_what_was_used() {
        let usage = json!({"prompt_tokens": 1000, "completion_tokens": 5});
        let tiny = call(usage, json!({"max_tokens": 1, "max_output_tokens": 800}));
        assert_eq!(output_tokens(&tiny), 800, "the largest limit named");
        let negative = call(json!({}), json!({"max_tokens": -5}));
        assert_eq!(output_tokens(&negative), 0);
        let long = call(json!({"completion_tokens": 5000}), json!({}));
        assert_eq!(output_tokens(&long), 5000, "more than the default, in full");
        assert_eq!(
            output_tokens(&call(json!({}), json!({}))),
            DEFAULT_OUTPUT_TOKENS
        );
        let unsaid = call(json!({}), json!({"input": "x".repeat(400)}));
        let bytes = unsaid.request.to_string().len() as i64;
        assert_eq!(input_tokens(&unsaid), (bytes + 3) / 4);
        assert_eq!(input_tokens(&tiny), 1000);
    }

    #[test]
    fn a_cost_not_even_a_ceiling_bounds_puts_only_a_limited_run_over() {
        let openai = ProviderConfig::default().resolve(Some("openai")).unwrap();
        let limited = Budget::new(Some(1.0));
        let unpriced = linked(&limited, (&openai, &spoke()), None, false);
        assert!(!unpriced.estimated, "{unpriced:?}");
        let why = limited.forced().unwrap();
        assert!(why.contains("is unknown"), "{why}");
        let unlimited = Budget::new(None);
        linked(&unlimited, (&openai, &spoke()), None, true);
        assert_eq!(unlimited.over(), None, "nothing to keep to");
    }

    #[test]
    fn an_openrouter_model_is_bounded_by_its_vendors_prices() {
        let router = ProviderConfig::default()
            .resolve(Some("openrouter"))
            .unwrap();
        let usage = json!({"prompt_tokens": 1000, "completion_tokens": 5});
        let p = call(
            usage,
            json!({"model": "anthropic/claude-x", "max_tokens": 100}),
        );
        let charge = linked(&Budget::new(Some(1.0)), (&router, &p), None, true);
        let (inp, out) = crate::prices::table().highest("anthropic").unwrap();
        let ceiling = (1000.0 * inp + 100.0 * out) / 1e6;
        assert_eq!((charge.usd, charge.estimated), (Some(ceiling), true));
    }
}
