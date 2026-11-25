//! The span of each kind of step: a model call (OpenTelemetry GenAI semantic conventions, with
//! OpenInference's names for Arize Phoenix and Langfuse's observation type), a tool call with
//! the rows it read and wrote, a reasoning step and the run's check. Content (prompts,
//! completions, tool arguments and results, reasoning text) only when the target takes it.

use chrono::{DateTime, Duration, Utc};
use serde_json::{Map, Value};

use super::span::{ids, text_of, AttrsBuilder, Failed, Kind, SpanEvent, SpanOut};
use crate::model::*;

/// What every step's span needs: the trace, the branch its ids are scoped to, and whether
/// content goes with it.
pub struct Ctx<'a> {
    pub trace: String,
    pub branch: &'a str,
    pub content: bool,
    /// What `World::tool_info` said at the run's start: `{name: {effect, part?}}`.
    pub tools: &'a Map<String, Value>,
}

impl Ctx<'_> {
    /// The id of the span of event `seq`.
    pub fn seq_id(&self, seq: i64) -> String {
        ids::span(self.branch, &format!("seq:{seq}"))
    }

    pub fn span(&self, e: &Event, name: String, parent: &str, start: DateTime<Utc>) -> SpanOut {
        SpanOut {
            trace_id: self.trace.clone(),
            span_id: self.seq_id(e.seq),
            parent: Some(parent.to_string()),
            name,
            kind: Kind::Internal,
            start: start.min(e.at),
            end: e.at,
            attrs: Default::default(),
            events: vec![],
            error: None,
        }
    }
}

/// `e.at` less `ms` milliseconds, when known.
pub fn back(at: DateTime<Utc>, ms: Option<f64>) -> Option<DateTime<Utc>> {
    let ms = ms.filter(|m| m.is_finite() && *m >= 0.0)?;
    Some(at - Duration::microseconds((ms * 1000.0) as i64))
}

/// The text a model answered: chat completions' first choice, Anthropic Messages' text blocks,
/// or OpenAI Responses' output text.
fn completion_text(r: &Value) -> Option<String> {
    if let Some(c) = r["choices"][0]["message"]["content"].as_str() {
        return Some(c.to_string());
    }
    let blocks = r["content"].as_array().or_else(|| r["output"].as_array())?;
    let texts = blocks.iter().flat_map(|b| {
        let inner = b["content"].as_array().cloned().unwrap_or_default();
        std::iter::once(b.clone()).chain(inner)
    });
    let text: Vec<String> = texts
        .filter_map(|b| b["text"].as_str().map(str::to_string))
        .collect();
    (!text.is_empty()).then(|| text.join("\n"))
}

/// Why the model stopped: each choice's `finish_reason`, Messages' `stop_reason`, Responses'
/// `status`.
fn finish_reasons(r: &Value) -> Vec<String> {
    let choices = r["choices"].as_array().into_iter().flatten();
    let mut out: Vec<String> = choices
        .filter_map(|c| c["finish_reason"].as_str().map(str::to_string))
        .collect();
    for k in ["stop_reason", "status"] {
        if let (true, Some(s)) = (out.is_empty(), r[k].as_str()) {
            out.push(s.to_string());
        }
    }
    out
}

/// The tools the model asked for, by name.
fn tool_calls_asked(r: &Value) -> Vec<String> {
    let chat = r["choices"][0]["message"]["tool_calls"].as_array();
    let chat = chat.into_iter().flatten();
    let named = chat.filter_map(|c| c["function"]["name"].as_str());
    let blocks = r["content"].as_array().or_else(|| r["output"].as_array());
    let used = blocks
        .into_iter()
        .flatten()
        .filter(|b| matches!(b["type"].as_str(), Some("tool_use" | "function_call")));
    let used = used.filter_map(|b| b["name"].as_str());
    named.chain(used).map(str::to_string).collect()
}

/// The request's messages, with Messages' `system` first: `(role, content)`.
fn messages(req: &Value) -> Vec<(String, String)> {
    let mut out = vec![];
    if let Some(s) = req.get("system").filter(|s| !s.is_null()) {
        out.push(("system".to_string(), text_of(s)));
    }
    let listed = req["messages"]
        .as_array()
        .or_else(|| req["input"].as_array());
    for m in listed.into_iter().flatten() {
        let role = m["role"].as_str().unwrap_or("user").to_string();
        out.push((role, text_of(&m["content"])));
    }
    out
}

/// `llm.input_messages.{i}.message.{role,content}` (OpenInference) and the same as one JSON.
fn input_attrs(mut a: AttrsBuilder, req: &Value) -> AttrsBuilder {
    for (i, (role, content)) in messages(req).into_iter().take(50).enumerate() {
        let k = format!("llm.input_messages.{i}.message");
        a = a
            .str(&format!("{k}.role"), role)
            .str(&format!("{k}.content"), content);
    }
    let input = req.get("messages").or_else(|| req.get("input"));
    match input {
        Some(m) => a
            .str("input.value", text_of(m))
            .str("input.mime_type", "application/json"),
        None => a,
    }
}

fn output_attrs(a: AttrsBuilder, resp: &Value) -> AttrsBuilder {
    match completion_text(resp) {
        Some(t) => a
            .str("output.value", &t)
            .str("llm.output_messages.0.message.role", "assistant")
            .str("llm.output_messages.0.message.content", t),
        None => a,
    }
}

/// Why a call failed, when its response says so.
fn call_error(resp: &Value) -> Option<Failed> {
    let e = resp.get("error").filter(|e| !e.is_null())?;
    let msg = e["message"].as_str().map(str::to_string);
    Some(Failed {
        message: msg.unwrap_or_else(|| text_of(e)),
    })
}

fn usage_attrs(a: AttrsBuilder, usage: &Value) -> AttrsBuilder {
    let n = |k: &str| usage.get(k).and_then(Value::as_i64);
    let (input, output, total) = (
        n("prompt_tokens"),
        n("completion_tokens"),
        n("total_tokens"),
    );
    a.opt_int("gen_ai.usage.input_tokens", input)
        .opt_int("gen_ai.usage.output_tokens", output)
        .opt_int("llm.token_count.prompt", input)
        .opt_int("llm.token_count.completion", output)
        .opt_int(
            "llm.token_count.total",
            total.or_else(|| Some(input? + output?)),
        )
}

/// What a model call says of itself: the GenAI conventions', OpenInference's and Langfuse's
/// names for the same things, and Shakedown's cost and timing.
fn model_attrs(p: &ModelCallPayload, model: Option<&str>) -> AttrsBuilder {
    let (req, resp) = (&p.request, &p.response);
    let provider = p.provider.as_deref();
    let max = req["max_tokens"]
        .as_i64()
        .or(req["max_completion_tokens"].as_i64());
    AttrsBuilder::default()
        .str("gen_ai.operation.name", "chat")
        .opt_str("gen_ai.system", provider)
        .opt_str("gen_ai.provider.name", provider)
        .opt_str("gen_ai.request.model", req["model"].as_str())
        .opt_str("gen_ai.response.model", resp["model"].as_str())
        .opt_str("gen_ai.response.id", resp["id"].as_str())
        .opt_float("gen_ai.request.temperature", req["temperature"].as_f64())
        .opt_int("gen_ai.request.max_tokens", max)
        .list("gen_ai.response.finish_reasons", finish_reasons(resp))
        .list("shakedown.model.tool_calls", tool_calls_asked(resp))
        .opt_float("gen_ai.usage.cost", p.cost_usd)
        .opt_float("shakedown.cost_usd", p.cost_usd)
        .opt_str("shakedown.cost_source", p.cost_source.as_deref())
        .opt_float("shakedown.model.duration_ms", p.duration_ms)
        .str("openinference.span.kind", "LLM")
        .str("langfuse.observation.type", "generation")
        .opt_str("llm.model_name", model)
        .opt_str("llm.provider", provider)
        .opt_str("llm.system", provider)
}

/// The span of model call `e`, under `parent`, starting `start` when its duration is unknown.
pub fn model_call(c: &Ctx, e: &Event, parent: &str, start: DateTime<Utc>) -> SpanOut {
    let p: ModelCallPayload = serde_json::from_value(e.payload.clone()).unwrap_or_default();
    let (req, resp) = (&p.request, &p.response);
    let model = req["model"].as_str().or(resp["model"].as_str());
    let started = back(e.at, p.duration_ms).unwrap_or(start);
    let name = format!("chat {}", model.unwrap_or("model"));
    let mut s = c.span(e, name, parent, started);
    s.kind = Kind::Client;
    let a = usage_attrs(model_attrs(&p, model), &p.usage);
    s.attrs = a
        .content(c.content, |a| output_attrs(input_attrs(a, req), resp))
        .build();
    if c.content {
        s.events = content_events(e.at, req, resp);
    }
    s.error = call_error(resp);
    s
}

/// The prompt and completion as span events (GenAI semantic conventions' event form).
fn content_events(at: DateTime<Utc>, req: &Value, resp: &Value) -> Vec<SpanEvent> {
    let prompt = req.get("messages").or_else(|| req.get("input"));
    let prompt = prompt.map(|m| ("gen_ai.content.prompt", "gen_ai.prompt", text_of(m)));
    let done = completion_text(resp).map(|t| ("gen_ai.content.completion", "gen_ai.completion", t));
    let ev = |(name, key, text): (&str, &str, String)| SpanEvent {
        name: name.to_string(),
        at,
        attrs: AttrsBuilder::default().str(key, text).build(),
    };
    prompt.into_iter().chain(done).map(ev).collect()
}

/// The keys of a tool call's `reads` or `writes` (a list of keys, or an object keyed by them).
fn keys_of(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a
            .iter()
            .map(|k| k.as_str().map_or_else(|| k.to_string(), str::to_string))
            .collect(),
        Value::Object(o) => o.keys().cloned().collect(),
        _ => vec![],
    }
}

/// What a tool call says of itself: the tool, its part, the rows it read and wrote.
fn tool_attrs(c: &Ctx, p: &ToolCallPayload) -> AttrsBuilder {
    let info = c.tools.get(&p.name);
    let (reads, writes) = (keys_of(&p.reads), keys_of(&p.writes));
    AttrsBuilder::default()
        .str("gen_ai.operation.name", "execute_tool")
        .str("gen_ai.tool.name", &p.name)
        .str("gen_ai.tool.type", "function")
        .opt_str("gen_ai.tool.call.id", p.tool_call_id.as_deref())
        .str("openinference.span.kind", "TOOL")
        .str("langfuse.observation.type", "tool")
        .str("tool.name", &p.name)
        .opt_str("shakedown.tool.part", p.part.as_deref())
        .opt_str(
            "shakedown.tool.effect",
            info.and_then(|i| i["effect"].as_str()),
        )
        .int("shakedown.db.rows_read", reads.len() as i64)
        .int("shakedown.db.rows_written", writes.len() as i64)
        .list("shakedown.db.keys_read", reads)
        .list("shakedown.db.keys_written", writes)
        .opt_int("shakedown.world.clock_ms", p.clock_ms)
        .opt_float("shakedown.tool.duration_ms", p.duration_ms)
}

/// The span of tool call `e`, under `parent`.
pub fn tool_call(c: &Ctx, e: &Event, parent: &str, start: DateTime<Utc>) -> SpanOut {
    let p: ToolCallPayload = serde_json::from_value(e.payload.clone()).unwrap_or_default();
    let started = back(e.at, p.duration_ms).unwrap_or(start);
    let mut s = c.span(e, format!("execute_tool {}", p.name), parent, started);
    let (args, result) = (text_of(&p.args), text_of(&p.result));
    let a = tool_attrs(c, &p).content(c.content, |a| {
        a.str("gen_ai.tool.call.arguments", &args)
            .str("tool.parameters", &args)
            .str("input.value", args.clone())
            .str("gen_ai.tool.call.result", &result)
            .str("output.value", result.clone())
    });
    s.attrs = a.build();
    s.error = p.error.map(|message| Failed { message });
    s
}

/// The span of reasoning step `e`, under `parent` (its model call's span when it names one).
pub fn reasoning(c: &Ctx, e: &Event, parent: &str) -> SpanOut {
    let p: ReasoningPayload = serde_json::from_value(e.payload.clone()).unwrap_or_default();
    let mut s = c.span(e, "reasoning".into(), parent, e.at);
    let kind = serde_json::to_value(p.kind).ok();
    let a = AttrsBuilder::default()
        .str("openinference.span.kind", "CHAIN")
        .str("langfuse.observation.type", "span")
        .opt_str(
            "shakedown.reasoning.kind",
            kind.as_ref().and_then(Value::as_str),
        )
        .opt_str("shakedown.reasoning.source", Some(&p.source))
        .bool("shakedown.reasoning.truncated", p.truncated)
        .content(c.content, |a| {
            a.str("shakedown.content.reasoning", &p.text)
                .str("output.value", &p.text)
        });
    s.attrs = a.build();
    s
}

/// Each named check of the run as an event.
fn named_checks(checks: &[parts::proto::NamedCheck], at: DateTime<Utc>) -> Vec<SpanEvent> {
    let event = |k: &parts::proto::NamedCheck| SpanEvent {
        name: "shakedown.check".into(),
        at,
        attrs: AttrsBuilder::default()
            .str("shakedown.check.name", &k.name)
            .bool("shakedown.check.passed", k.passed)
            .bool("shakedown.check.safety", k.safety)
            .opt_str("shakedown.check.detail", Some(&k.detail))
            .build(),
    };
    checks.iter().map(event).collect()
}

/// The span of the run's check `e`, under the branch: whether the world ended as it should,
/// each named check an event.
pub fn check(c: &Ctx, e: &Event, parent: &str, start: DateTime<Utc>) -> SpanOut {
    let p: CheckPayload = serde_json::from_value(e.payload.clone()).unwrap_or_default();
    let mut s = c.span(e, "evaluate".into(), parent, start);
    let differing = p.differing_rows.clone().unwrap_or_default();
    let checks = p.checks.clone().unwrap_or_default();
    let passed = checks.iter().filter(|k| k.passed).count() as i64;
    s.attrs = AttrsBuilder::default()
        .str("openinference.span.kind", "EVALUATOR")
        .str("langfuse.observation.type", "evaluator")
        .bool("shakedown.check.passed", p.passed)
        .int("shakedown.check.differing", differing.len() as i64)
        .list("shakedown.check.differing_rows", differing)
        .list("shakedown.check.unchecked", p.unchecked.clone())
        .int("shakedown.check.named", checks.len() as i64)
        .int("shakedown.check.named_passed", passed)
        .build();
    s.events = named_checks(&checks, e.at);
    if !p.passed {
        let message = "the world did not end as the task expects".into();
        s.error = Some(Failed { message });
    }
    s
}
