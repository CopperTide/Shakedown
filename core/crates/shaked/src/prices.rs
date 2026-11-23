//! The list prices of `docs/prices.toml`, built into shaked, for estimating what tokens cost when
//! the provider reports no cost.

use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::Value;

use crate::model::AgentKind;

/// One model's list price on one provider, per million tokens.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Price {
    pub provider: String,
    pub model: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    /// Cache writes (5-minute) and cache hits, when listed; else 1.25 and 0.1 times the input
    /// price, the provider's stated multipliers.
    #[serde(default)]
    pub cache_write_per_mtok: Option<f64>,
    #[serde(default)]
    pub cache_read_per_mtok: Option<f64>,
    /// Above this many prompt tokens in one call the call is priced at the long-context rates
    /// (input and output; cache rates scale with the input price).
    #[serde(default)]
    pub long_context_above: Option<i64>,
    #[serde(default)]
    pub long_input_per_mtok: Option<f64>,
    #[serde(default)]
    pub long_output_per_mtok: Option<f64>,
    /// Where the price was read.
    pub source: String,
    /// When it was read (YYYY-MM-DD).
    pub as_of: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct Prices {
    #[serde(default)]
    price: Vec<Price>,
}

const BUILT_IN: &str = include_str!("../../../../docs/prices.toml");

static TABLE: LazyLock<Prices> =
    LazyLock::new(|| Prices::parse(BUILT_IN).expect("docs/prices.toml is valid: a test parses it"));

/// The built-in list prices.
pub fn table() -> &'static Prices {
    &TABLE
}

impl Prices {
    pub fn parse(text: &str) -> Result<Prices, toml::de::Error> {
        toml::from_str(text)
    }

    /// The price of `model` on `provider`, if the table has it.
    pub fn find(&self, provider: &str, model: &str) -> Option<&Price> {
        self.price
            .iter()
            .find(|p| p.provider == provider && p.model == model)
    }

    /// The most an input token and an output token of `provider` can cost, per million tokens,
    /// over every model the table lists for it (an input token at its dearest: a cache write at
    /// the long-context rate); None when it lists none.
    pub fn highest(&self, provider: &str) -> Option<(f64, f64)> {
        let of = self.price.iter().filter(|p| p.provider == provider);
        let rates = |p: &Price| {
            let long = p.long_input_per_mtok.unwrap_or(0.0).max(p.input_per_mtok);
            let input = long.max(p.cache_write_rate() * long / p.input_per_mtok);
            let output = p.long_output_per_mtok.unwrap_or(0.0).max(p.output_per_mtok);
            (input, output)
        };
        of.map(rates)
            .reduce(|(i, o), (i2, o2)| (i.max(i2), o.max(o2)))
    }

    /// `find`, for pricing a total of many calls: not a price with a long-context tier, whose
    /// rate depends on each call's prompt.
    pub fn find_flat(&self, provider: &str, model: &str) -> Option<&Price> {
        self.find(provider, model)
            .filter(|p| p.long_context_above.is_none())
    }
}

/// One model call's tokens: fresh input, cache writes, cache hits and output.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CallTokens {
    pub input: i64,
    pub cache_write: i64,
    pub cache_read: i64,
    pub output: i64,
}

/// The label of a cost shaked worked out at these list prices.
pub const LIST_PRICE_ESTIMATE: &str = "list_price_estimate";

impl CallTokens {
    /// The tokens of a call's canonical `usage`: `prompt_tokens` is the whole input, of which the
    /// cache hits (`cache_read_input_tokens`, or `prompt_tokens_details.cached_tokens`) and the
    /// cache writes (`cache_creation_input_tokens`) are priced apart.
    pub fn of_usage(u: &Value) -> CallTokens {
        let n = |v: &Value| v.as_i64().unwrap_or(0);
        let read = n(&u["prompt_tokens_details"]["cached_tokens"]);
        let read = read.max(n(&u["cache_read_input_tokens"]));
        let written = n(&u["cache_creation_input_tokens"]);
        CallTokens {
            input: (n(&u["prompt_tokens"]) - read - written).max(0),
            cache_read: read,
            cache_write: written,
            output: n(&u["completion_tokens"]),
        }
    }

    /// The whole prompt the call was sent with.
    pub fn prompt(&self) -> i64 {
        self.input + self.cache_write + self.cache_read
    }
}

const CACHE_WRITE_RATE: f64 = 1.25;
const CACHE_READ_RATE: f64 = 0.1;

impl Price {
    /// What a cache write costs per million tokens: the listed price, else the provider's stated
    /// multiple of the input price.
    fn cache_write_rate(&self) -> f64 {
        (self.cache_write_per_mtok).unwrap_or(self.input_per_mtok * CACHE_WRITE_RATE)
    }

    /// What `prompt` input and `completion` output tokens cost at this price.
    pub fn cost(&self, prompt: i64, completion: i64) -> f64 {
        let per = |n: i64, p: f64| n as f64 * p / 1_000_000.0;
        per(prompt, self.input_per_mtok) + per(completion, self.output_per_mtok)
    }

    /// What one call costs at this price, cache writes and hits priced apart, and the whole call
    /// at the long-context rates when its prompt is over the threshold.
    pub fn call_cost(&self, t: &CallTokens) -> f64 {
        let long = self.long_context_above.is_some_and(|n| t.prompt() > n);
        let (input, output) = match (long, self.long_input_per_mtok, self.long_output_per_mtok) {
            (true, Some(i), Some(o)) => (i, o),
            _ => (self.input_per_mtok, self.output_per_mtok),
        };
        let scale = input / self.input_per_mtok;
        let write = self.cache_write_rate();
        let read = self
            .cache_read_per_mtok
            .unwrap_or(self.input_per_mtok * CACHE_READ_RATE);
        let per = |n: i64, p: f64| n as f64 * p / 1_000_000.0;
        per(t.input, input)
            + per(t.cache_write, write * scale)
            + per(t.cache_read, read * scale)
            + per(t.output, output)
    }
}

/// What one call of `model` on `provider` costs at list price; None when the table lacks it.
pub fn call_cost(provider: &str, model: &str, t: &CallTokens) -> Option<f64> {
    table().find(provider, model).map(|p| p.call_cost(t))
}

/// The provider whose prices apply to an agent: the CLI's vendor, or an open agent's provider
/// (`openrouter` when it names none).
pub fn provider_of(kind: AgentKind, provider: Option<&str>) -> Option<String> {
    match kind {
        AgentKind::ClaudeCode => Some("anthropic".into()),
        AgentKind::Codex => Some("openai".into()),
        AgentKind::Open => Some(provider.unwrap_or("openrouter").to_string()),
        AgentKind::External => provider.map(str::to_string),
        AgentKind::Http => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_table_parses_and_names_its_sources() {
        let t = table();
        assert!(!t.price.is_empty());
        for p in &t.price {
            assert!(p.source.starts_with("https://"), "{p:?}");
            assert_eq!(p.as_of.len(), 10, "{p:?}");
            assert!(p.input_per_mtok > 0.0 && p.output_per_mtok > 0.0, "{p:?}");
        }
    }

    #[test]
    fn a_price_is_per_million_tokens_of_each_kind() {
        let t = Prices::parse(
            r#"[[price]]
provider = "x"
model = "m"
input_per_mtok = 2.0
output_per_mtok = 10.0
source = "https://example.org"
as_of = "2026-11-04""#,
        )
        .unwrap();
        let p = t.find("x", "m").unwrap();
        assert_eq!(p.cost(1_000_000, 100_000), 3.0);
        assert!(t.find("x", "other").is_none() && t.find("y", "m").is_none());
        assert_eq!(
            provider_of(AgentKind::Open, None).as_deref(),
            Some("openrouter")
        );
        assert_eq!(
            provider_of(AgentKind::ClaudeCode, None).as_deref(),
            Some("anthropic")
        );
    }

    fn tokens(input: i64, cache_write: i64, cache_read: i64, output: i64) -> CallTokens {
        CallTokens {
            input,
            cache_write,
            cache_read,
            output,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    /// Cache hits and writes priced apart (Haiku 5.5: 0.1 input, 0.125 write, 0.01 read, 0.5
    /// output per million).
    #[test]
    fn a_call_prices_its_cache_apart() {
        let p = table().find("anthropic", "claude-haiku-5-5").unwrap();
        let want = (1_000.0 * 0.1 + 9_000.0 * 0.125 + 50_000.0 * 0.01 + 2_000.0 * 0.5) / 1e6;
        assert!(close(
            p.call_cost(&tokens(1_000, 9_000, 50_000, 2_000)),
            want
        ));
        let opus = table().find("anthropic", "claude-opus-5-5").unwrap();
        let hit = tokens(0, 0, 1_000_000, 0);
        assert!(
            close(opus.call_cost(&hit), 0.20),
            "a listed cache price wins"
        );
        assert_eq!(call_cost("anthropic", "nope", &hit), None);
    }

    /// Over 100K prompt tokens a Haiku 5.5 call is at 0.5 input (so 0.05 read) and 2.5 output.
    #[test]
    fn a_long_prompt_is_priced_at_the_long_context_rates() {
        let p = table().find("anthropic", "claude-haiku-5-5").unwrap();
        let want = (200_000.0 * 0.05 + 1_000.0 * 2.5) / 1e6;
        assert!(close(p.call_cost(&tokens(0, 0, 200_000, 1_000)), want));
        assert!(table().find_flat("anthropic", "claude-haiku-5-5").is_none());
    }

    #[test]
    fn a_haiku_call_is_priced_from_the_table() {
        let t = CallTokens {
            cache_read: 1_000_000,
            ..CallTokens::default()
        };
        assert!(call_cost("anthropic", "claude-haiku-5-5", &t).unwrap() > 0.0);
    }
}
