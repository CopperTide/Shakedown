//! A branch's record as a span tree, one trace per run:
//!
//! ```text
//! run (the run's first branch only)          experiment, suite, scenario, setup, world, task
//! └─ branch                                  status, outcome, fork; world events, pauses
//!    ├─ invoke_agent <name>  (one per turn)  model, tokens, cost; handoffs, messages, reports
//!    │  ├─ chat <model>                      GenAI + OpenInference: tokens, cost, finish reason
//!    │  │  └─ reasoning                      what the model thought (content)
//!    │  ├─ execute_tool <name>               rows read and written, args and result (content)
//!    │  └─ <the agent's own spans>           what its OTLP exporter sent during the run
//!    └─ evaluate                             the check: passed, differing rows, named checks
//! ```
//!
//! Only the branch's own events count (`seq >= start_seq`); a fork's spans hang under the run span
//! its first branch sent. Times are the record's.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::{
    imported,
    span::{ids, Attrs, AttrsBuilder, Failed, Kind, SpanEvent, SpanOut},
    steps::{self, Ctx},
};
use crate::{model::*, store::RunRecord};

/// A turn an agent is in: its span's index in the output.
struct Turn {
    at: usize,
}

struct Walk<'a> {
    c: Ctx<'a>,
    branch_span: String,
    out: Vec<SpanOut>,
    turns: HashMap<String, Turn>,
    /// The span of each model call, by its seq (a reasoning step names its call).
    calls: HashMap<i64, String>,
    /// When each agent's last step ended: where its next model call starts when its duration is
    /// unknown.
    last: HashMap<String, DateTime<Utc>>,
    imported: imported::Remap,
}

/// The branch's own events (a fork's inherited ones are its parent's), and when it ended (or
/// its last event, while it runs).
fn own_of(rec: &RunRecord) -> (Vec<&Event>, DateTime<Utc>) {
    let b = &rec.branch;
    let from = b.start_seq.unwrap_or(0);
    let own: Vec<&Event> = rec.events.iter().filter(|e| e.seq >= from).collect();
    let last = own.last().map(|e| e.at);
    let end = b.ended_at.or(last).unwrap_or(b.created_at);
    (own, end)
}

/// The run's span (on its first branch only) and the branch's.
fn heads(
    rec: &RunRecord,
    start: &RunStartPayload,
    trace: &str,
    end: DateTime<Utc>,
) -> Vec<SpanOut> {
    let run_span = ids::span(&rec.run_id, "run");
    let branch_span = ids::span(&rec.branch.id, "branch");
    let mut out = vec![];
    if rec.branch.parent_id.is_none() {
        out.push(run_span_of(rec, start, (trace, &run_span), end));
    }
    out.push(branch_span_of(
        rec,
        start,
        (trace, &branch_span, &run_span),
        end,
    ));
    out
}

/// The spans of `rec`, with content when `content`.
pub fn spans(rec: &RunRecord, content: bool) -> Vec<SpanOut> {
    let start = RunStartPayload::of(&rec.events).unwrap_or_default();
    let trace = ids::trace(&rec.run_id);
    let (own, end) = own_of(rec);
    let out = heads(rec, &start, &trace, end);
    let b = &rec.branch.id;
    let mut w = Walk {
        c: Ctx {
            trace,
            branch: b,
            content,
            tools: &start.tools,
        },
        branch_span: ids::span(b, "branch"),
        out,
        turns: HashMap::new(),
        calls: HashMap::new(),
        last: HashMap::new(),
        imported: imported::Remap::of(b, &own),
    };
    for e in own {
        w.step(e);
    }
    w.close_all(end);
    w.out
}

/// The experiment and suite the run belongs to.
fn group_attrs(a: AttrsBuilder, rec: &RunRecord) -> AttrsBuilder {
    let (x, s) = (rec.experiment.as_ref(), rec.suite.as_ref());
    a.opt_str("shakedown.experiment.id", x.map(|x| x.0.as_str()))
        .opt_str("shakedown.experiment.name", x.map(|x| x.1.as_str()))
        .opt_str("shakedown.suite.id", s.map(|s| s.0.as_str()))
        .opt_str("shakedown.suite.name", s.map(|s| s.1.as_str()))
}

/// What the run is of: on the run span, and on each branch span, so a fork's trace says it too.
fn run_attrs(rec: &RunRecord, start: &RunStartPayload) -> AttrsBuilder {
    let team = start.team.as_ref();
    let scenario_id = start.scenario.as_ref().and_then(|s| s["id"].as_str());
    let parts = start.world.iter().flatten().map(|p| p.part.clone());
    let a = AttrsBuilder::default()
        .str("shakedown.run.id", &rec.run_id)
        .opt_str("shakedown.run.name", rec.run_name.as_deref())
        .str("shakedown.task.id", &rec.task_id)
        .opt_str("shakedown.world", start.benchmark.as_deref())
        .list("shakedown.world.parts", parts)
        .opt_str("shakedown.setup.id", team.map(|t| t.id.as_str()))
        .opt_str("shakedown.setup.name", team.map(|t| t.name.as_str()))
        .opt_str("shakedown.scenario.id", scenario_id)
        .opt_str("shakedown.runner", start.runner.map(runner_name))
        .int("shakedown.seed", start.seed as i64)
        .opt_float("shakedown.max_cost_usd", start.max_cost_usd);
    group_attrs(a, rec)
}

fn runner_name(r: Runner) -> &'static str {
    match r {
        Runner::Chain => "chain",
        Runner::Concurrent => "concurrent",
        Runner::Dispatch => "dispatch",
        Runner::External => "external",
    }
}

fn run_span_of(
    rec: &RunRecord,
    start: &RunStartPayload,
    (trace, id): (&str, &str),
    end: DateTime<Utc>,
) -> SpanOut {
    let session = rec.experiment.as_ref().map_or(&rec.run_id, |x| &x.0);
    let name = rec
        .run_name
        .clone()
        .unwrap_or_else(|| format!("run {}", rec.task_id));
    let attrs = run_attrs(rec, start)
        .str("openinference.span.kind", "CHAIN")
        .str("langfuse.trace.name", &name)
        .str("langfuse.session.id", session)
        .str("session.id", session)
        .build();
    SpanOut {
        trace_id: trace.to_string(),
        span_id: id.to_string(),
        parent: None,
        name,
        kind: Kind::Internal,
        start: rec.run_created_at.min(rec.branch.created_at),
        end,
        attrs,
        events: vec![],
        error: None,
    }
}

fn branch_attrs(rec: &RunRecord, start: &RunStartPayload) -> AttrsBuilder {
    let b = &rec.branch;
    let mode = b
        .restore_mode
        .as_ref()
        .and_then(|m| serde_json::to_value(m).ok());
    run_attrs(rec, start)
        .str("shakedown.branch.id", &b.id)
        .str("shakedown.branch.status", b.status.as_str())
        .bool("shakedown.outcome.passed", b.status == BranchStatus::Passed)
        .opt_str("shakedown.branch.error", b.error.as_deref())
        .opt_str("shakedown.branch.parent_id", b.parent_id.as_deref())
        .opt_int("shakedown.branch.start_seq", b.start_seq)
        .opt_str("shakedown.branch.fork_note", b.fork_note.as_deref())
        .opt_str(
            "shakedown.branch.restore_mode",
            mode.as_ref().and_then(|m| m.as_str()),
        )
        .str("openinference.span.kind", "CHAIN")
}

/// Why a branch that ended badly did.
fn branch_error(b: &Branch) -> Option<Failed> {
    let failed = matches!(
        b.status,
        BranchStatus::Failed | BranchStatus::InfraError | BranchStatus::BudgetExceeded
    );
    let message = b
        .error
        .clone()
        .unwrap_or_else(|| b.status.as_str().to_string());
    failed.then_some(Failed { message })
}

fn branch_span_of(
    rec: &RunRecord,
    start: &RunStartPayload,
    (trace, id, run): (&str, &str, &str),
    end: DateTime<Utc>,
) -> SpanOut {
    SpanOut {
        trace_id: trace.to_string(),
        span_id: id.to_string(),
        parent: Some(run.to_string()),
        name: "branch".into(),
        kind: Kind::Internal,
        start: rec.branch.created_at,
        end,
        attrs: branch_attrs(rec, start).build(),
        events: vec![],
        error: branch_error(&rec.branch),
    }
}

/// An event's payload as a span event's attributes: `shakedown.<kind>.<field>` for each
/// scalar field, the text fields only with content.
fn as_event(e: &Event, content: bool) -> SpanEvent {
    let kind = e.kind.as_str();
    let mut a = AttrsBuilder::default().opt_str("shakedown.agent", e.agent.as_deref());
    for (k, v) in e.payload.as_object().into_iter().flatten() {
        let key = format!("shakedown.{kind}.{k}");
        let texty = matches!(
            k.as_str(),
            "text" | "note" | "task" | "summary" | "input" | "data"
        );
        a = match v {
            _ if texty && !content => a,
            serde_json::Value::String(s) => a.str(&key, s),
            serde_json::Value::Bool(b) => a.bool(&key, *b),
            serde_json::Value::Number(n) if n.is_i64() => a.opt_int(&key, n.as_i64()),
            serde_json::Value::Number(n) => a.opt_float(&key, n.as_f64()),
            serde_json::Value::Null => a,
            other if texty => a.str(&key, super::span::text_of(other)),
            _ => a,
        };
    }
    SpanEvent {
        name: format!("shakedown.{kind}"),
        at: e.at,
        attrs: a.build(),
    }
}

/// The name and attributes of the turn agent `agent` starts at `e` (its start, or a wake).
fn turn_attrs(e: &Event, agent: &str) -> (String, Attrs) {
    let p: AgentStartPayload = match e.kind {
        EventKind::AgentStart => serde_json::from_value(e.payload.clone()).unwrap_or_default(),
        _ => AgentStartPayload::default(),
    };
    let name = Some(p.name.as_str())
        .filter(|n| !n.is_empty())
        .unwrap_or(agent);
    let kind = p.kind.and_then(|k| serde_json::to_value(k).ok());
    let wake = (e.kind == EventKind::Wake)
        .then(|| e.payload["reason"].as_str())
        .flatten();
    let attrs = AttrsBuilder::default()
        .str("gen_ai.operation.name", "invoke_agent")
        .str("gen_ai.agent.id", agent)
        .str("gen_ai.agent.name", name)
        .opt_str("gen_ai.request.model", p.model.as_deref())
        .opt_str("gen_ai.provider.name", p.provider.as_deref())
        .opt_str(
            "shakedown.agent.kind",
            kind.as_ref().and_then(|k| k.as_str()),
        )
        .bool("shakedown.agent.external", p.external)
        .opt_str("shakedown.agent.wake_reason", wake)
        .str("openinference.span.kind", "AGENT")
        .str("langfuse.observation.type", "agent")
        .build();
    (name.to_string(), attrs)
}

impl Walk<'_> {
    fn step(&mut self, e: &Event) {
        use EventKind as K;
        match e.kind {
            K::AgentStart | K::Wake => self.open(e),
            K::AgentEnd | K::Idle => self.close(e),
            K::ModelCall => self.child(e, |w, parent, start| {
                let s = steps::model_call(&w.c, e, parent, start);
                w.calls.insert(e.seq, s.span_id.clone());
                s
            }),
            K::ToolCall => self.child(e, |w, parent, start| {
                steps::tool_call(&w.c, e, parent, start)
            }),
            K::Reasoning => self.reasoning(e),
            K::Spans => {
                let parent = self.turn_of(e);
                let spans = self.imported.spans(&self.c, e, &parent);
                self.out.extend(spans);
            }
            K::Check => {
                let start = self.last.values().max().copied().unwrap_or(e.at);
                let s = steps::check(&self.c, e, &self.branch_span, start);
                self.out.push(s);
            }
            _ => self.note(e),
        }
        if let Some(a) = &e.agent {
            self.last.insert(a.clone(), e.at);
        }
    }

    /// An event that is no step of its own: an event on its agent's turn (handoffs, messages,
    /// reports), else on the branch (world events, pauses); bookkeeping is left out.
    fn note(&mut self, e: &Event) {
        use EventKind as K;
        if matches!(e.kind, K::RunStart | K::SavePoint | K::WorldView) {
            return;
        }
        let on = self.turn_of(e);
        let ev = as_event(e, self.c.content);
        self.event_on(&on, ev);
    }

    /// Opens agent `e.agent`'s turn at `e` (closing one still open there).
    fn open(&mut self, e: &Event) {
        let Some(agent) = e.agent.clone() else {
            return;
        };
        if let Some(t) = self.turns.remove(&agent) {
            self.out[t.at].end = e.at;
        }
        let at = self.out.len();
        self.out.push(self.turn_span(e, &agent));
        self.turns.insert(agent, Turn { at });
    }

    fn turn_span(&self, e: &Event, agent: &str) -> SpanOut {
        let (name, attrs) = turn_attrs(e, agent);
        SpanOut {
            trace_id: self.c.trace.clone(),
            span_id: ids::span(self.c.branch, &format!("turn:{agent}:{}", e.seq)),
            parent: Some(self.branch_span.clone()),
            name: format!("invoke_agent {name}"),
            kind: Kind::Internal,
            start: e.at,
            end: e.at,
            attrs,
            events: vec![],
            error: None,
        }
    }

    /// Closes agent `e.agent`'s turn at `e`: how it ended, what it used.
    fn close(&mut self, e: &Event) {
        let Some(agent) = e.agent.clone() else {
            return;
        };
        let i = self.turn_index(e);
        self.turns.remove(&agent);
        let s = &mut self.out[i];
        s.end = s.end.max(e.at);
        let ok = e.payload["ok"].as_bool().unwrap_or(true);
        let p = &e.payload;
        let mut a = AttrsBuilder(std::mem::take(&mut s.attrs))
            .bool("shakedown.agent.ok", ok)
            .opt_int("gen_ai.usage.input_tokens", p["prompt_tokens"].as_i64())
            .opt_int(
                "gen_ai.usage.output_tokens",
                p["completion_tokens"].as_i64(),
            )
            .opt_int("shakedown.agent.tokens", p["tokens"].as_i64())
            .opt_float("shakedown.cost_usd", p["cost_usd"].as_f64())
            .opt_float("shakedown.agent.seconds", p["seconds"].as_f64());
        if self.c.content {
            a = a.opt_str("output.value", p["text"].as_str());
        }
        s.attrs = a.build();
        if !ok {
            let why = p["error"].as_str().unwrap_or("the agent's turn failed");
            s.error = Some(Failed {
                message: why.to_string(),
            });
        }
    }

    /// The index of `e.agent`'s open turn, opening one at `e` when it has none.
    fn turn_index(&mut self, e: &Event) -> usize {
        let agent = e.agent.clone().unwrap_or_default();
        if !self.turns.contains_key(&agent) {
            let at = self.out.len();
            self.out.push(self.turn_span(e, &agent));
            self.turns.insert(agent.clone(), Turn { at });
        }
        self.turns[&agent].at
    }

    /// The span id of `e.agent`'s open turn (the branch's span for an event of no agent).
    fn turn_of(&mut self, e: &Event) -> String {
        if e.agent.is_none() {
            return self.branch_span.clone();
        }
        let i = self.turn_index(e);
        self.out[i].span_id.clone()
    }

    /// A step of `e.agent`'s turn, made by `make` with its parent and where it starts when its
    /// duration is unknown (the agent's last step).
    fn child(&mut self, e: &Event, make: impl FnOnce(&mut Self, &str, DateTime<Utc>) -> SpanOut) {
        let parent = self.turn_of(e);
        let agent = e.agent.clone().unwrap_or_default();
        let start = self.last.get(&agent).copied().unwrap_or(e.at);
        let s = make(self, &parent, start);
        self.stretch(&parent, s.start, s.end);
        self.out.push(s);
    }

    fn reasoning(&mut self, e: &Event) {
        let call = e.payload["model_call_seq"].as_i64();
        let parent = match call.and_then(|seq| self.calls.get(&seq)) {
            Some(id) => id.clone(),
            None => self.turn_of(e),
        };
        let s = steps::reasoning(&self.c, e, &parent);
        self.out.push(s);
    }

    /// Widens span `id` to cover `start..end`.
    fn stretch(&mut self, id: &str, start: DateTime<Utc>, end: DateTime<Utc>) {
        if let Some(s) = self.out.iter_mut().find(|s| s.span_id == id) {
            s.start = s.start.min(start);
            s.end = s.end.max(end);
        }
    }

    fn event_on(&mut self, id: &str, ev: SpanEvent) {
        let at = ev.at;
        if let Some(s) = self.out.iter_mut().find(|s| s.span_id == id) {
            s.events.push(ev);
            s.end = s.end.max(at);
        }
    }

    /// Ends the turns still open when the branch ended.
    fn close_all(&mut self, end: DateTime<Utc>) {
        for (_, t) in self.turns.drain() {
            let s = &mut self.out[t.at];
            s.end = s.end.max(end);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const EVENTS: &str = include_str!("../../tests/fixtures/otel/run_events.json");
    /// Rewritten by `SHAKEDOWN_BLESS=1 cargo test -p shaked --lib otel::tree`.
    const GOLDEN: &str = "tests/fixtures/otel/run_spans.json";

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().expect("a fixture time")
    }

    /// A recorded chain run of two agents: a model call with its reasoning, two tool calls (one
    /// refused), the first agent's own spans, a handoff, a world event and a failed check.
    fn record() -> RunRecord {
        let events: Vec<Event> = serde_json::from_str(EVENTS).expect("the fixture's events");
        let mut branch = crate::testkit::branch(BranchStatus::Failed);
        branch.id = events[0].branch_id.clone();
        branch.run_id = "0123456789abcdef0123456789abcdef".into();
        branch.created_at = at("2026-11-20T10:00:00Z");
        branch.ended_at = Some(at("2026-11-20T10:00:15Z"));
        RunRecord {
            run_id: branch.run_id.clone(),
            run_name: Some("Late refund".into()),
            task_id: "27".into(),
            run_created_at: at("2026-11-20T09:59:59Z"),
            suite: Some(("s1".into(), "Refund policy (Support desk)".into())),
            experiment: Some(("x1".into(), "Refund policy".into())),
            branch,
            events,
        }
    }

    #[test]
    fn a_recorded_run_maps_to_its_golden_span_tree() {
        let got = serde_json::to_string_pretty(&spans(&record(), true)).unwrap() + "\n";
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);
        if std::env::var_os("SHAKEDOWN_BLESS").is_some() {
            std::fs::write(&path, &got).unwrap();
        }
        let want = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            got == want,
            "differs from {GOLDEN} (SHAKEDOWN_BLESS=1 rewrites it):\n{got}"
        );
    }

    #[test]
    fn every_span_hangs_under_the_run_and_ids_are_stable() {
        let all = spans(&record(), true);
        let named = |n: &str| all.iter().find(|s| s.name == n).unwrap();
        let parent = |n: &str| named(n).parent.clone().unwrap();
        assert_eq!(parent("reasoning"), named("chat gpt-4o-mini").span_id);
        assert_eq!(parent("rerank"), named("retrieve policy").span_id);
        assert_eq!(
            parent("retrieve policy"),
            named("invoke_agent Triage").span_id
        );
        assert!(named("execute_tool refund").error.is_some());
        assert_eq!(spans(&record(), true), all, "the same spans every time");
    }

    #[test]
    fn spans_are_one_tree_with_unique_ids() {
        let all = spans(&record(), true);
        let ids: HashSet<&str> = all.iter().map(|s| s.span_id.as_str()).collect();
        assert_eq!(ids.len(), all.len(), "unique ids");
        let roots: Vec<&str> = all
            .iter()
            .filter(|s| s.parent.is_none())
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(roots, ["Late refund"]);
        for s in &all {
            let p = s.parent.as_deref();
            assert!(
                p.is_none_or(|p| ids.contains(p)),
                "{}'s parent is sent",
                s.name
            );
            assert!(s.start <= s.end, "{}", s.name);
        }
    }

    #[test]
    fn without_content_no_prompt_completion_argument_or_text_leaves() {
        let all = spans(&record(), false);
        let text = serde_json::to_string(&all).unwrap();
        let said = [
            "Order #W1 arrived late",
            "Looking it up",
            "late_days",
            "Look up the order",
            "late delivery refund",
            "Refund it: 3 days late",
            "SLA missed",
        ];
        for s in said {
            assert!(!text.contains(s), "{s} left without content");
        }
        assert_eq!(all.len(), spans(&record(), true).len(), "the same steps");
        assert!(
            text.contains("\"gen_ai.usage.input_tokens\":120"),
            "numbers stay"
        );
    }

    #[test]
    fn a_fork_sends_only_its_own_steps_under_the_run_span() {
        let mut rec = record();
        rec.branch.parent_id = Some("parent".into());
        rec.branch.start_seq = Some(9);
        let all = spans(&rec, true);
        assert!(
            all.iter().all(|s| s.parent.is_some()),
            "the run span is its first branch's"
        );
        let run = ids::span(&rec.run_id, "run");
        assert_eq!(all[0].parent.as_deref(), Some(run.as_str()));
        assert!(
            !all.iter().any(|s| s.name == "invoke_agent Triage"),
            "inherited"
        );
        assert!(all.iter().any(|s| s.name == "invoke_agent Refunds"));
    }
}
