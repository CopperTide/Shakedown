//! A branch's numbers for deployment estimates: the wall time measured, Shakedown's own overhead in
//! it, the time estimated in production (measured minus overhead), and tokens and cost per agent
//! and model. Only the branch's own events count (`seq >= start_seq`), as in compare: the
//! inherited prefix belongs to its parent.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::{
    model::*,
    overhead::Overhead,
    prices::{self, Prices},
    providers::LOCAL_COST_SOURCE,
    world::models::is_world_actor,
};

pub const REPORTED: &str = "reported";
pub const ESTIMATE: &str = "list-price estimate";
pub const NO_CHARGE: &str = "no API charge";
pub const UNKNOWN: &str = "unknown";
pub const MIXED: &str = "mixed";

/// A cost and where it comes from: what the provider reported, a list-price estimate (with the
/// price's source and date), no API charge (a local model), or unknown.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cost {
    pub usd: Option<f64>,
    pub label: String,
    /// The provider's cost source, or the URL a list price was read at.
    pub source: Option<String>,
    /// The date of a list price.
    pub as_of: Option<String>,
}

impl Cost {
    fn unknown() -> Cost {
        Cost::labelled(None, UNKNOWN)
    }

    fn labelled(usd: Option<f64>, label: &str) -> Cost {
        Cost {
            usd,
            label: label.into(),
            source: None,
            as_of: None,
        }
    }
}

/// What one agent used while it ran with one model.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentUse {
    pub agent: String,
    pub kind: Option<AgentKind>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub tokens: i64,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub cost: Cost,
    /// The model calls recorded for it (through the gateway, or reported by its client).
    pub model_calls: i64,
    /// The largest `prompt_tokens` of those calls: the most context it was sent with.
    pub peak_prompt_tokens: Option<i64>,
    /// How many times its client compacted its context (reported).
    pub compactions: i64,
    /// Shakedown's overhead recorded on this agent's own events, in seconds.
    pub overhead_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Metrics {
    /// Wall time from the branch's creation to its end (or to now while it runs).
    pub measured_s: f64,
    /// Shakedown's own time in it, per kind, in milliseconds.
    pub overhead_ms: Overhead,
    pub overhead_s: f64,
    /// False for runs recorded before overhead was measured: then there is no estimate.
    pub overhead_recorded: bool,
    /// `measured_s - overhead_s` (never below 0), when overhead was recorded.
    pub estimated_s: Option<f64>,
    pub tokens: i64,
    pub cost: Cost,
    pub agents: Vec<AgentUse>,
}

/// Tokens and costs seen for one agent run, before its cost is settled.
#[derive(Default)]
struct Tally {
    ended: bool,
    tokens: i64,
    prompt: Option<i64>,
    completion: Option<i64>,
    reported: Option<f64>,
    source: Option<String>,
    calls: CallTally,
    overhead_ms: f64,
}

/// What the agent's model calls (through the gateway) add up to.
#[derive(Default)]
struct CallTally {
    prompt: i64,
    completion: i64,
    total: i64,
    cost: Option<f64>,
    all_costed: bool,
    any: bool,
    source: Option<String>,
    count: i64,
    peak: Option<i64>,
    compactions: i64,
    /// What each client-reported call (by its message id) added, so a call reported again
    /// replaces its earlier report instead of counting twice.
    by_id: HashMap<String, [i64; 3]>,
}

fn opt_add(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    Some(a.unwrap_or(0) + b?)
}

impl CallTally {
    /// A compaction a client reported: no tokens of its own.
    fn compacted(&mut self) {
        self.compactions += 1;
    }

    /// One model call. A call whose message id was reported before replaces that report: a
    /// client may report a call more than once (Claude Code repeats a message per content block).
    fn add(&mut self, p: &ModelCallPayload, id: Option<&str>) {
        let n = |k: &str| p.usage.get(k).and_then(Value::as_i64).unwrap_or(0);
        let (prompt, completion) = (n("prompt_tokens"), n("completion_tokens"));
        let mine = [
            prompt,
            completion,
            n("total_tokens").max(prompt + completion),
        ];
        let earlier = id.and_then(|i| self.by_id.insert(i.to_string(), mine));
        let [p0, c0, t0] = earlier.unwrap_or_default();
        self.count += i64::from(earlier.is_none());
        if p.usage
            .get("prompt_tokens")
            .and_then(Value::as_i64)
            .is_some()
        {
            self.peak = Some(self.peak.map_or(prompt, |m| m.max(prompt)));
        }
        if !self.any {
            self.all_costed = true;
        }
        self.any = true;
        (self.prompt, self.completion) =
            (self.prompt + prompt - p0, self.completion + completion - c0);
        self.total += mine[2] - t0;
        self.all_costed &= p.cost_usd.is_some();
        self.cost = add_cost(Some(self.cost.unwrap_or(0.0)), p.cost_usd);
        self.source = self.source.take().or_else(|| p.cost_source.clone());
    }
}

impl Tally {
    fn end(&mut self, e: &AgentEndPayload) {
        self.ended = true;
        self.tokens += e.tokens.unwrap_or(0);
        self.prompt = opt_add(self.prompt, e.prompt_tokens);
        self.completion = opt_add(self.completion, e.completion_tokens);
        if let Some(c) = e.cost_usd {
            self.reported = Some(self.reported.unwrap_or(0.0) + c);
        }
        self.source = self.source.take().or_else(|| e.cost_source.clone());
    }

    /// Tokens of the run: its ends', or its model calls' when it never ended (or kept no split).
    /// Calls its client reported one by one, by message id, are Shakedown's own record of them, so
    /// their sums win over the totals the client added up itself at the end.
    fn settle(&mut self) {
        let calls = &self.calls;
        let by_calls = !calls.by_id.is_empty();
        if (!self.ended || by_calls) && calls.any {
            self.tokens = calls.total;
        }
        if !self.ended && calls.any && calls.all_costed {
            self.reported = calls.cost;
            self.source = calls.source.clone();
        }
        if (self.prompt.is_none() || by_calls) && calls.any {
            (self.prompt, self.completion) = (Some(calls.prompt), Some(calls.completion));
        }
    }
}

/// The label of a cost an agent's run came with, by its source: no API charge on a local model, an
/// estimate when shaked worked any of it out at list prices, else reported.
fn label_of(source: Option<&str>) -> &'static str {
    match source {
        Some(LOCAL_COST_SOURCE) => NO_CHARGE,
        Some(s) if s.contains(prices::LIST_PRICE_ESTIMATE) => ESTIMATE,
        _ => REPORTED,
    }
}

/// The cost of one agent run: reported, else estimated from a list price, else unknown.
fn cost_of(t: &Tally, u: &AgentUse, table: &Prices) -> Cost {
    if let Some(usd) = t.reported {
        let mut c = Cost::labelled(Some(usd), label_of(t.source.as_deref()));
        c.source = t.source.clone();
        return c;
    }
    let price = u
        .kind
        .and_then(|k| prices::provider_of(k, u.provider.as_deref()));
    let price = price.zip(u.model.as_deref());
    let price = price.and_then(|(p, m)| table.find_flat(&p, m));
    match (price, t.prompt, t.completion) {
        (Some(p), Some(i), Some(o)) if t.tokens > 0 || i + o > 0 => Cost {
            usd: Some(p.cost(i, o)),
            label: ESTIMATE.into(),
            source: Some(p.source.clone()),
            as_of: Some(p.as_of.clone()),
        },
        _ => Cost::unknown(),
    }
}

/// One agent run, from its AgentStart.
fn agent_use(e: &Event) -> AgentUse {
    let start = e.decode::<AgentStartPayload>().unwrap_or_default();
    AgentUse {
        agent: e.agent.clone().unwrap_or_default(),
        kind: start.kind,
        model: start.model,
        provider: start.provider,
        tokens: 0,
        prompt_tokens: None,
        completion_tokens: None,
        cost: Cost::unknown(),
        model_calls: 0,
        peak_prompt_tokens: None,
        compactions: 0,
        overhead_s: 0.0,
    }
}

fn parsed<P: serde::de::DeserializeOwned>(e: &Event) -> Option<P> {
    serde_json::from_value(e.payload.clone()).ok()
}

/// The run an agent's event `e` belongs to: its latest in `runs`, else, for a model call or an
/// end, one carried over from its `AgentStart` in `inherited` (a fork's agent resumed without
/// starting again). A world actor's model calls (which start no agent) make a run of their own
/// per model.
fn run_of<'r>(
    runs: &'r mut Vec<(AgentUse, Tally)>,
    inherited: &[&Event],
    e: &Event,
) -> Option<&'r mut Tally> {
    let i = match position_of(runs, e) {
        Some(i) => i,
        None => {
            runs.push((new_use(inherited, e)?, Tally::default()));
            runs.len() - 1
        }
    };
    Some(&mut runs[i].1)
}

/// Where `e`'s run is in `runs`: its agent's latest, a world actor's of the same model.
fn position_of(runs: &[(AgentUse, Tally)], e: &Event) -> Option<usize> {
    let agent = e.agent.as_deref();
    let same = |u: &AgentUse| Some(u.agent.as_str()) == agent;
    if !is_world_actor(agent) {
        return runs.iter().rposition(|(u, _)| same(u));
    }
    let model = e.payload["request"]["model"].as_str();
    runs.iter()
        .rposition(|(u, _)| same(u) && u.model.as_deref() == model)
}

/// The run `e` starts when none has it yet: a world actor's model call, or a model call or end
/// of an agent whose start is inherited.
fn new_use(inherited: &[&Event], e: &Event) -> Option<AgentUse> {
    let agent = e.agent.as_deref();
    if is_world_actor(agent) && e.kind == EventKind::ModelCall {
        return Some(world_use(e));
    }
    matches!(e.kind, EventKind::AgentEnd | EventKind::ModelCall).then_some(())?;
    let start = (inherited.iter().rev())
        .find(|e| e.kind == EventKind::AgentStart && e.agent.as_deref() == agent)?;
    Some(agent_use(start))
}

/// A world actor's use of one model, from its first call: kind `world`, the model it asked for
/// and the provider that answered.
fn world_use(e: &Event) -> AgentUse {
    let text = |v: &Value| v.as_str().map(str::to_string);
    AgentUse {
        kind: Some(AgentKind::World),
        model: text(&e.payload["request"]["model"]),
        provider: text(&e.payload["provider"]),
        ..agent_use(e)
    }
}

/// The agent runs of `events`, each with what it used. `inherited` are the events before them
/// (a fork's prefix): an agent that goes on in `events` without starting again counts from its
/// start there.
fn agent_uses(events: &[&Event], inherited: &[&Event], table: &Prices) -> Vec<AgentUse> {
    let mut runs: Vec<(AgentUse, Tally)> = vec![];
    for e in events {
        if e.kind == EventKind::AgentStart {
            runs.push((agent_use(e), Tally::default()));
            continue;
        }
        let Some(t) = run_of(&mut runs, inherited, e) else {
            continue;
        };
        t.overhead_ms += event_overhead(e);
        match e.kind {
            EventKind::AgentEnd => parsed(e).iter().for_each(|p| t.end(p)),
            EventKind::ModelCall if e.payload.get("compaction").is_some() => t.calls.compacted(),
            EventKind::ModelCall => {
                let id = e.payload.get("message_id").and_then(Value::as_str);
                parsed(e).iter().for_each(|p| t.calls.add(p, id));
            }
            _ => {}
        }
    }
    runs.into_iter().map(|(u, t)| finish(u, t, table)).collect()
}

fn finish(mut u: AgentUse, mut t: Tally, table: &Prices) -> AgentUse {
    t.settle();
    u.cost = cost_of(&t, &u, table);
    (u.tokens, u.prompt_tokens, u.completion_tokens) = (t.tokens, t.prompt, t.completion);
    (u.model_calls, u.peak_prompt_tokens) = (t.calls.count, t.calls.peak);
    (u.compactions, u.overhead_s) = (t.calls.compactions, t.overhead_ms / 1000.0);
    u
}

/// The overhead recorded on one event, in milliseconds.
fn event_overhead(e: &Event) -> f64 {
    let o = e.payload.get("overhead_ms").cloned();
    let o = o.and_then(|o| serde_json::from_value::<Overhead>(o).ok());
    o.map_or(0.0, |o| o.total())
}

/// The branch's cost: the sum when every agent's is known, labelled with their one label (or
/// `mixed`); unknown as soon as one is.
fn total_cost(agents: &[AgentUse]) -> Cost {
    let mut labels: Vec<&str> = agents.iter().map(|a| a.cost.label.as_str()).collect();
    labels.dedup();
    let usd = agents
        .iter()
        .try_fold(0.0, |s, a| a.cost.usd.map(|c| s + c));
    match (usd, labels.as_slice()) {
        (None, _) | (_, []) => Cost::unknown(),
        (Some(usd), [one]) => {
            let first = &agents[0].cost;
            let same = agents.iter().all(|a| a.cost.source == first.source);
            Cost {
                source: first.source.clone().filter(|_| same),
                as_of: first.as_of.clone().filter(|_| same),
                ..Cost::labelled(Some(usd), one)
            }
        }
        (Some(usd), _) => Cost::labelled(Some(usd), MIXED),
    }
}

/// The overhead recorded on `events`, and whether any was.
fn overhead_of(events: &[&Event]) -> (Overhead, bool) {
    let mut sum = Overhead::default();
    let mut any = false;
    for e in events {
        let Some(o) = e.payload.get("overhead_ms") else {
            continue;
        };
        if let Ok(o) = serde_json::from_value::<Overhead>(o.clone()) {
            sum.merge(&o);
            any = true;
        }
    }
    (sum, any)
}

/// The metrics of `branch` from its `events`, at `now` while it runs, priced with `table`.
pub fn metrics(branch: &Branch, events: &[Event], table: &Prices, now: DateTime<Utc>) -> Metrics {
    let from = branch.start_seq.unwrap_or(0);
    let (own, inherited): (Vec<&Event>, Vec<&Event>) = events.iter().partition(|e| e.seq >= from);
    let end = branch.ended_at.unwrap_or(now);
    let measured_s = ((end - branch.created_at).num_milliseconds() as f64 / 1000.0).max(0.0);
    let (overhead_ms, overhead_recorded) = overhead_of(&own);
    let overhead_s = overhead_ms.total() / 1000.0;
    let agents = agent_uses(&own, &inherited, table);
    Metrics {
        measured_s,
        overhead_s,
        overhead_recorded,
        estimated_s: overhead_recorded.then(|| (measured_s - overhead_s).max(0.0)),
        overhead_ms,
        tokens: agents.iter().map(|a| a.tokens).sum(),
        cost: total_cost(&agents),
        agents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::testkit::{self, event as ev};

    fn branch(start_seq: Option<i64>, secs: i64) -> Branch {
        let b = testkit::branch(BranchStatus::Passed);
        Branch {
            start_seq,
            ended_at: Some(b.created_at + chrono::Duration::seconds(secs)),
            ..b
        }
    }

    fn table() -> Prices {
        Prices::parse(
            r#"[[price]]
provider = "anthropic"
model = "m"
input_per_mtok = 2.0
output_per_mtok = 10.0
source = "https://example.org/prices"
as_of = "2026-11-04""#,
        )
        .unwrap()
    }

    fn start(seq: i64, agent: &str, kind: &str, model: &str, provider: Option<&str>) -> Event {
        let mut p = json!({"name": agent, "kind": kind, "model": model});
        if let Some(pr) = provider {
            p["provider"] = json!(pr);
        }
        ev(seq, Some(agent), EventKind::AgentStart, p)
    }

    fn end(seq: i64, agent: &str, split: Option<(i64, i64)>, cost: Option<(f64, &str)>) -> Event {
        let (i, o) = split.unwrap_or((60, 40));
        let mut p = json!({"ok": true, "error": null, "tokens": i + o, "seconds": 1.0,
                           "cost_usd": cost.map(|c| c.0), "cost_source": cost.map(|c| c.1)});
        if split.is_some() {
            (p["prompt_tokens"], p["completion_tokens"]) = (json!(i), json!(o));
        }
        ev(seq, Some(agent), EventKind::AgentEnd, p)
    }

    #[test]
    fn a_save_point_is_no_step() {
        let without = vec![
            ev(1, None, EventKind::RunStart, json!({})),
            start(2, "a", "open", "m", Some("anthropic")),
            end(3, "a", Some((1_000, 100)), None),
        ];
        let point = json!({"reason": "model_call", "saves": {"box": "s"}, "paused_ms": 5});
        let mut with = without.clone();
        with.insert(2, ev(3, None, EventKind::SavePoint, point));
        with[3].seq = 4;
        let (b, now) = (branch(None, 10), Utc::now());
        assert_eq!(
            metrics(&b, &with, &table(), now),
            metrics(&b, &without, &table(), now)
        );
    }

    #[test]
    fn the_estimate_is_measured_time_minus_overhead() {
        let evs = vec![
            ev(
                1,
                None,
                EventKind::RunStart,
                json!({"overhead_ms": {"restore": 1500.0}}),
            ),
            start(2, "a", "open", "m", Some("anthropic")),
            ev(
                3,
                Some("a"),
                EventKind::ToolCall,
                json!({"name": "t", "overhead_ms": {"save": 400.0, "record": 100.0}}),
            ),
            end(4, "a", Some((1_000_000, 100_000)), None),
        ];
        let m = metrics(&branch(None, 10), &evs, &table(), Utc::now());
        assert_eq!((m.measured_s, m.overhead_s), (10.0, 2.0));
        assert_eq!(m.estimated_s, Some(8.0));
        assert_eq!((m.overhead_ms.restore, m.overhead_ms.save), (1500.0, 400.0));
        assert_eq!(m.agents.len(), 1);
        assert_eq!(m.agents[0].cost.usd, Some(3.0));
        assert_eq!(m.cost.label, ESTIMATE);
        assert_eq!(m.cost.as_of.as_deref(), Some("2026-11-04"));
        assert_eq!(m.cost.source.as_deref(), Some("https://example.org/prices"));
    }

    #[test]
    fn old_runs_have_no_estimate_and_forks_count_only_their_own_steps() {
        let evs = vec![
            start(1, "a", "open", "m", Some("anthropic")),
            end(2, "a", None, Some((0.5, "openrouter"))),
            start(3, "b", "claude_code", "sonnet", None),
            end(4, "b", None, Some((0.25, "api_equivalent_estimate"))),
        ];
        let m = metrics(&branch(None, 4), &evs, &table(), Utc::now());
        assert!(!m.overhead_recorded && m.estimated_s.is_none());
        assert_eq!(
            (m.tokens, m.cost.usd, m.cost.label.as_str()),
            (200, Some(0.75), REPORTED)
        );
        let fork = metrics(&branch(Some(3), 4), &evs, &table(), Utc::now());
        assert_eq!((fork.tokens, fork.agents.len()), (100, 1));
        // A fork whose agent goes on without starting again: its end still counts.
        let resumed = metrics(&branch(Some(4), 4), &evs, &table(), Utc::now());
        let a = &resumed.agents[0];
        assert_eq!(
            (a.agent.as_str(), a.tokens, a.cost.usd),
            ("b", 100, Some(0.25))
        );
    }

    #[test]
    fn a_cost_without_a_price_is_unknown_and_local_models_cost_nothing() {
        let evs = vec![
            start(1, "a", "open", "not-listed", Some("openai")),
            end(2, "a", Some((10, 5)), None),
            start(3, "b", "open", "llama", Some("local:box")),
            end(4, "b", Some((10, 5)), Some((0.0, LOCAL_COST_SOURCE))),
        ];
        let m = metrics(&branch(None, 4), &evs, &table(), Utc::now());
        assert_eq!(m.agents[0].cost.label, UNKNOWN);
        assert_eq!(m.agents[1].cost.label, NO_CHARGE);
        assert_eq!((m.cost.usd, m.cost.label.as_str()), (None, UNKNOWN));
        assert_eq!(m.tokens, 30);
    }

    /// A cost shaked worked out at list prices (an open agent's end, a turn stopped before Claude
    /// Code said its cost) is an estimate, never a reported one; Claude Code's own figure stays
    /// reported.
    #[test]
    fn a_list_price_cost_is_labelled_an_estimate() {
        let evs = vec![
            start(1, "a", "open", "claude-x", Some("anthropic")),
            end(2, "a", Some((10, 5)), Some((0.5, "list_price_estimate"))),
            start(3, "b", "claude_code", "sonnet", None),
            cut_off_end(4, "b"),
            start(5, "c", "claude_code", "sonnet", None),
            end(6, "c", Some((10, 5)), Some(CLAUDE_CODES_OWN)),
        ];
        let m = metrics(&branch(None, 6), &evs, &table(), Utc::now());
        let labels: Vec<&str> = m.agents.iter().map(|a| a.cost.label.as_str()).collect();
        assert_eq!(labels, [ESTIMATE, ESTIMATE, REPORTED]);
        let source = m.agents[0].cost.source.as_deref();
        assert_eq!(source, Some("list_price_estimate"));
    }

    /// The cost Claude Code said a session cost.
    const CLAUDE_CODES_OWN: (f64, &str) = (0.25, "api_equivalent_estimate");

    /// `agent`'s end after earlier turns reported their cost and its last was stopped.
    fn cut_off_end(seq: i64, agent: &str) -> Event {
        let cost = (0.75, "api_equivalent_estimate+list_price_estimate");
        end(seq, agent, Some((10, 5)), Some(cost))
    }

    /// A model call `agent`'s client reported: `prompt` tokens of context, or a compaction.
    fn reported(seq: i64, prompt: Option<i64>) -> Event {
        let mut p = json!({"request": null, "response": null, "cost_usd": null,
                           "cost_source": null, "usage": {}, "overhead_ms": {"mcp": 250.0}});
        match prompt {
            Some(n) => p["usage"] = json!({"prompt_tokens": n, "completion_tokens": 1}),
            None => p["compaction"] = json!({"pre_tokens": 190000}),
        }
        ev(seq, Some("c"), EventKind::ModelCall, p)
    }

    #[test]
    fn reported_calls_give_each_agent_its_peak_context_compactions_and_overhead() {
        let call = |seq, n| reported(seq, Some(n));
        let compaction = reported(4, None);
        let evs = vec![
            start(1, "c", "external", "m", None),
            call(2, 1200),
            call(3, 150_000),
            compaction,
            call(5, 40_000),
            end(6, "c", Some((191_200, 3)), None),
        ];
        let m = metrics(&branch(None, 9), &evs, &table(), Utc::now());
        let a = &m.agents[0];
        assert_eq!(
            (a.model_calls, a.peak_prompt_tokens, a.compactions),
            (3, Some(150_000), 1)
        );
        assert!((a.overhead_s - 1.0).abs() < 1e-9, "{}", a.overhead_s);
        assert_eq!(a.tokens, 191_203);
    }

    /// A call `c`'s client reported under message id `id`, with `prompt` and `out` tokens.
    fn reported_as(seq: i64, id: &str, prompt: i64, out: i64) -> Event {
        let p = json!({"request": null, "response": null, "cost_usd": null, "cost_source": null,
                       "usage": {"prompt_tokens": prompt, "completion_tokens": out},
                       "message_id": id, "reported_by": "client"});
        ev(seq, Some("c"), EventKind::ModelCall, p)
    }

    #[test]
    fn a_call_reported_again_replaces_its_report_and_the_calls_sums_win() {
        let evs = vec![
            start(1, "c", "external", "m", None),
            reported_as(2, "m1", 1000, 1),
            reported_as(3, "m1", 1000, 1),
            reported_as(4, "m2", 3000, 1),
            reported_as(5, "m2", 3000, 40),
            end(6, "c", Some((9_999, 2)), None),
        ];
        let m = metrics(&branch(None, 9), &evs, &table(), Utc::now());
        let a = &m.agents[0];
        assert_eq!((a.model_calls, a.peak_prompt_tokens), (2, Some(3000)));
        assert_eq!(
            (a.prompt_tokens, a.completion_tokens),
            (Some(4000), Some(41))
        );
        assert_eq!(a.tokens, 4041);
    }

    /// A model call of the world actor `actor` with `model`, `tokens` and `cost`.
    fn world_call(seq: i64, actor: &str, model: &str, cost: f64) -> Event {
        let p = json!({"request": {"model": model}, "response": {}, "provider": "openrouter",
                       "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10},
                       "cost_usd": cost, "cost_source": "openrouter"});
        ev(seq, Some(actor), EventKind::ModelCall, p)
    }

    #[test]
    fn a_world_actors_calls_are_its_own_use_per_model_and_count_in_the_branch() {
        let evs = vec![
            start(1, "a", "open", "m", Some("anthropic")),
            world_call(2, "world.shop.customer", "cust", 0.25),
            world_call(3, "world.shop.customer", "cust", 0.25),
            world_call(4, "world.scoring", "judge", 0.5),
            end(5, "a", Some((60, 40)), Some((1.0, "openrouter"))),
        ];
        let m = metrics(&branch(None, 9), &evs, &table(), Utc::now());
        let of = |id: &str| m.agents.iter().find(|a| a.agent == id).unwrap();
        let customer = of("world.shop.customer");
        assert_eq!(customer.kind, Some(AgentKind::World));
        assert_eq!(customer.model.as_deref(), Some("cust"));
        assert_eq!((customer.tokens, customer.model_calls), (20, 2));
        assert_eq!(customer.cost.usd, Some(0.5));
        assert_eq!(of("world.scoring").model.as_deref(), Some("judge"));
        assert_eq!((m.tokens, m.cost.usd), (130, Some(2.0)));
        let v = serde_json::to_value(customer).unwrap();
        assert_eq!(v["kind"], "world");
    }

    #[test]
    fn an_agent_that_never_ended_counts_its_model_calls() {
        let call = |seq, cost: Option<f64>| {
            ev(
                seq,
                Some("x"),
                EventKind::ModelCall,
                json!({"request": {}, "response": {},
               "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10},
               "cost_usd": cost, "cost_source": cost.map(|_| "openrouter")}),
            )
        };
        let evs = vec![
            start(1, "x", "open", "q", None),
            call(2, Some(0.1)),
            call(3, Some(0.2)),
        ];
        let m = metrics(&branch(None, 1), &evs, &table(), Utc::now());
        let a = &m.agents[0];
        assert_eq!(
            (a.tokens, a.prompt_tokens, a.completion_tokens),
            (20, Some(14), Some(6))
        );
        assert!((a.cost.usd.unwrap() - 0.3).abs() < 1e-9);
        let evs = vec![
            start(1, "x", "open", "q", None),
            call(2, Some(0.1)),
            call(3, None),
        ];
        let m = metrics(&branch(None, 1), &evs, &table(), Utc::now());
        assert_eq!(m.agents[0].cost.label, UNKNOWN);
    }
}
