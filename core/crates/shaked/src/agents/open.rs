use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use super::{
    session::{end_error, BoxFuture, Session, SessionStart, Started, Turn},
    AgentResult, AgentRun, Driver, FailureKind,
};
use crate::{
    app::AppState,
    exec::{tool_schemas, BranchCtx, ToolExec, FINISH, HANDOFF},
    gateway::Api,
    model::{add_cost, AgentSpec, TokenUsage},
    providers::{keys, Provider, ProviderConfig},
};

/// How many model calls an API-mode agent's turn makes at most, unless
/// `SHAKEDOWN_AGENT_MAX_MODEL_CALLS` says otherwise (see [`max_model_calls`]).
pub const MAX_MODEL_CALLS: usize = 40;
/// The idle note of a dispatch turn cut at [`max_model_calls`].
pub const CALL_LIMIT_NOTE: &str = "turn ended at the model-call limit";
/// How many of a session's latest turns keep their tool results whole (`SHAKEDOWN_CONTEXT_TURNS`).
pub const CONTEXT_TURNS: usize = 4;
/// A tool result at most this long is never trimmed: a placeholder would be as long.
const TRIM_MIN_CHARS: usize = 400;
/// How many times a model call is tried again when its provider is overloaded (statuses 503 and
/// 529); every try is recorded.
pub const MAX_RETRIES: u32 = 3;
/// How many times a rate-limited call (429) is tried again: with [`MAX_RETRY_WAIT`], enough to
/// outlast a per-minute limit (1 + 2 + 4 + 8 + 16 + 30 s with no wait asked).
pub const MAX_RATE_LIMIT_RETRIES: u32 = 6;
const RETRYABLE: [u16; 3] = [429, 503, 529];
const RATE_LIMITED: u16 = 429;
/// The longest wait before a retry, whatever the provider asks.
pub const MAX_RETRY_WAIT: Duration = Duration::from_secs(30);

/// How the loop ended badly: the model misbehaved, or something broke around it.
enum Fault {
    Model(String),
    Infra(String),
    /// The run's spend estimate passed its limit (`crate::budget`).
    Budget(String),
    /// The turn made [`max_model_calls`] calls without ending; what the agent said last.
    CallLimit(String),
}

impl Fault {
    /// The error and how the agent's failure counts (`None`: the harness's).
    fn split(self) -> (String, Option<FailureKind>) {
        match self {
            Fault::Model(e) => (e, Some(FailureKind::Model)),
            Fault::Budget(e) => (e, Some(FailureKind::Budget)),
            Fault::CallLimit(_) => (
                format!("agent exceeded {} model calls", max_model_calls()),
                Some(FailureKind::Model),
            ),
            Fault::Infra(e) => (e, None),
        }
    }
}

impl From<String> for Fault {
    fn from(s: String) -> Fault {
        Fault::Infra(s)
    }
}

impl From<&str> for Fault {
    fn from(s: &str) -> Fault {
        Fault::Infra(s.to_string())
    }
}

/// Our own agent loop: every model call goes through the recording gateway.
pub struct OpenDriver {
    // Weak: `AppState` owns the drivers, so a strong reference would be a cycle.
    state: Weak<AppState>,
}

impl OpenDriver {
    pub fn new(state: Arc<AppState>) -> OpenDriver {
        OpenDriver {
            state: Arc::downgrade(&state),
        }
    }
}

fn openai_tools(branch: &BranchCtx, agent: &str) -> Vec<Value> {
    tool_schemas(branch, agent)
        .into_iter()
        .map(|s| {
            json!({"type": "function", "function": {
                "name": s["name"], "description": s["description"], "parameters": s["inputSchema"]}})
        })
        .collect()
}

fn initial_messages(run: &AgentRun) -> Vec<Value> {
    let system = json!({"role": "system", "content": run.spec.prompt});
    match &run.resume {
        None => vec![system, json!({"role": "user", "content": run.input})],
        Some(msgs) => {
            let mut out = vec![system];
            out.extend(msgs.iter().filter(|m| m["role"] != "system").cloned());
            out
        }
    }
}

impl Driver for OpenDriver {
    fn run<'a>(&'a self, run: AgentRun) -> Pin<Box<dyn Future<Output = AgentResult> + Send + 'a>> {
        Box::pin(self.run_inner(run))
    }

    fn session(&self, start: SessionStart) -> Started<'_> {
        Box::pin(async move {
            let state = self.state.upgrade().ok_or("server is shutting down")?;
            // The caller's own keys, as for a run: an agent may name one (`key:<name>`).
            let providers = keys::config_for(&state, &start.branch.caller).await;
            let lp =
                Loop::new(state, start.branch, &start.spec, &providers).map_err(|f| f.split().0)?;
            let system = json!({"role": "system", "content": start.spec.prompt});
            let session: Box<dyn Session> = Box::new(OpenSession {
                lp,
                messages: vec![system],
                context_turns: context_turns(),
                totals: Totals::new(),
                busy: Duration::ZERO,
                error: None,
            });
            Ok::<_, String>(session)
        })
    }
}

/// An open agent alive across turns: shaked keeps its conversation, and each turn adds a user
/// message to it.
struct OpenSession {
    lp: Loop,
    messages: Vec<Value>,
    /// Turns whose tool results stay whole; older ones are trimmed (0: never).
    context_turns: usize,
    totals: Totals,
    busy: Duration,
    error: Option<String>,
}

impl OpenSession {
    async fn run_turn(&mut self, input: String) -> Turn {
        let since = Instant::now();
        self.messages
            .push(json!({"role": "user", "content": input}));
        let trimmed = trim_context(&mut self.messages, self.context_turns);
        let done = self
            .lp
            .until_done(&mut self.messages, &mut self.totals)
            .await;
        self.busy += since.elapsed();
        let mut turn = session_turn(done);
        turn.trimmed = trimmed;
        if !turn.ok {
            self.error = turn.error.clone();
        }
        turn
    }
}

impl Session for OpenSession {
    fn turn<'a>(&'a mut self, input: String) -> BoxFuture<'a, Turn> {
        Box::pin(self.run_turn(input))
    }

    fn end(self: Box<Self>, cut: bool) -> BoxFuture<'static, AgentResult> {
        let error = end_error(self.error, cut);
        let result = self.totals.result(error, None, self.busy.as_secs_f64());
        Box::pin(async move { result })
    }
}

/// Token and cost totals over a run's model calls.
struct Totals {
    tokens: [i64; 3],
    /// `None` once a call's cost is unknown.
    cost: Option<f64>,
    /// The label of the cost: the provider's, once a call was made.
    source: Option<&'static str>,
}

impl Totals {
    fn new() -> Totals {
        Totals {
            tokens: [0; 3],
            cost: Some(0.0),
            source: None,
        }
    }

    /// Adds a call of `model` on `provider` with `usage`, at the price the gateway charges it.
    fn add(&mut self, provider: &Provider, model: &str, usage: &Value) {
        for (total, k) in
            self.tokens
                .iter_mut()
                .zip(["prompt_tokens", "completion_tokens", "total_tokens"])
        {
            *total += usage[k].as_i64().unwrap_or(0);
        }
        self.cost = add_cost(self.cost, provider.call_cost(model, usage));
        self.source = Some(provider.call_cost_source());
    }

    /// The agent's result: these totals, with `error` and `failure`, over `seconds`.
    fn result(
        self,
        error: Option<String>,
        failure: Option<FailureKind>,
        seconds: f64,
    ) -> AgentResult {
        let [prompt_tokens, completion_tokens, total_tokens] = self.tokens;
        AgentResult {
            ok: error.is_none(),
            error,
            failure,
            tokens: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens,
            },
            cost_source: self.cost.and(self.source).map(str::to_string),
            cost_usd: self.cost,
            seconds,
        }
    }
}

/// The arguments of a tool call: `{}` for none, `None` when they are not a JSON object.
fn parse_tool_args(raw: &str) -> Option<Value> {
    if raw.trim().is_empty() {
        return Some(json!({}));
    }
    serde_json::from_str::<Value>(raw)
        .ok()
        .filter(Value::is_object)
}

fn tool_message(id: &str, content: String) -> Value {
    json!({"role": "tool", "tool_call_id": id, "content": content})
}

/// What the gateway answered: its status, the `retry-after` seconds it passed on, its body.
struct GatewayAnswer {
    status: reqwest::StatusCode,
    retry_after: Option<u64>,
    text: String,
}

/// The agent's gateway: where it is and the agent's token for it.
struct Gateway {
    url: String,
    token: String,
}

async fn post(http: &reqwest::Client, gw: &Gateway, body: &Value) -> Result<GatewayAnswer, Fault> {
    let resp = http
        .post(&gw.url)
        .bearer_auth(&gw.token)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("gateway request failed: {e}"))?;
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok()?.parse().ok());
    let text = resp
        .text()
        .await
        .map_err(|e| format!("gateway response unreadable: {e}"))?;
    Ok(GatewayAnswer {
        status,
        retry_after,
        text,
    })
}

/// One model call through the gateway, tried again (after [`retry_wait`]) while the provider is
/// overloaded ([`MAX_RETRIES`] times) or rate-limiting ([`MAX_RATE_LIMIT_RETRIES`] times); the
/// parsed response.
async fn model_call(
    http: &reqwest::Client,
    gw: &Gateway,
    body: &Value,
    retry_base: Duration,
) -> Result<Value, Fault> {
    let began = Instant::now();
    let mut retries = 0;
    loop {
        let a = post(http, gw, body).await?;
        if a.status.is_success() {
            return Ok(
                serde_json::from_str(&a.text).map_err(|e| format!("bad model response: {e}"))?
            );
        }
        let status = a.status.as_u16();
        if !RETRYABLE.contains(&status) {
            return Err(failure(a.status, &a.text).into());
        }
        if retries == retries_for(status) {
            let gave_up = failure(a.status, &a.text);
            return Err(tried(&gave_up, status, retries + 1, began.elapsed()).into());
        }
        tokio::time::sleep(retry_wait(retry_base, retries, a.retry_after)).await;
        retries += 1;
    }
}

/// How many retries a call that got `status` (one of [`RETRYABLE`]) gets.
fn retries_for(status: u16) -> u32 {
    match status {
        RATE_LIMITED => MAX_RATE_LIMIT_RETRIES,
        _ => MAX_RETRIES,
    }
}

/// The wait before retry number `retries + 1`: what the provider asked, else `base` doubled
/// for each retry already made; at most [`MAX_RETRY_WAIT`].
fn retry_wait(base: Duration, retries: u32, asked: Option<u64>) -> Duration {
    let wait = asked.map_or_else(|| base.saturating_mul(1 << retries), Duration::from_secs);
    wait.min(MAX_RETRY_WAIT)
}

/// A call's failure after `tries` tries over `took`: a rate limit says so first.
fn tried(failure: &str, status: u16, tries: u32, took: Duration) -> String {
    let over = format!("{tries} tries over {} s", took.as_secs());
    match status {
        RATE_LIMITED => format!("rate-limited by the model provider ({over}): {failure}"),
        _ => format!("{failure} ({over})"),
    }
}

/// Why a model call failed, in plain words: the provider's own error message when it gave one.
fn failure(status: reqwest::StatusCode, text: &str) -> String {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let error = &v["body"]["error"];
    let detail = [&error["message"], error, &v["error"]]
        .into_iter()
        .find_map(Value::as_str)
        .unwrap_or(text);
    format!("model call failed ({}): {detail}", status.as_u16())
}

/// The recording gateway's chat address for `agent` on `branch`.
fn gateway_url(state: &AppState, branch: &str, agent: &str) -> String {
    let base = super::endpoint(&state.config.self_base, "gateway", branch, agent);
    format!("{base}/v1/{}", Api::Chat.path())
}

/// How a session's turn ended. A turn that reached [`max_model_calls`] ends like any other, with a
/// note (the agent waits for its next wake), where a one-shot run fails.
fn session_turn(done: Result<String, Fault>) -> Turn {
    match done {
        Ok(text) => Turn::done(text),
        Err(Fault::CallLimit(text)) => Turn {
            note: Some(CALL_LIMIT_NOTE.to_string()),
            ..Turn::done(text)
        },
        Err(f) => {
            let (e, failure) = f.split();
            Turn::failed(e, failure)
        }
    }
}

/// How many model calls an API-mode agent's turn makes at most: `SHAKEDOWN_AGENT_MAX_MODEL_CALLS`,
/// read once, else [`MAX_MODEL_CALLS`].
pub fn max_model_calls() -> usize {
    static MAX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let read = || {
        call_limit(
            std::env::var("SHAKEDOWN_AGENT_MAX_MODEL_CALLS")
                .ok()
                .as_deref(),
        )
    };
    *MAX.get_or_init(read)
}

/// The call limit a setting gives: a whole number above 0, else [`MAX_MODEL_CALLS`].
fn call_limit(setting: Option<&str>) -> usize {
    let n = setting.and_then(|v| v.trim().parse().ok());
    n.filter(|n: &usize| *n > 0).unwrap_or(MAX_MODEL_CALLS)
}

/// `SHAKEDOWN_CONTEXT_TURNS`, read when a session starts: a whole number, else [`CONTEXT_TURNS`].
fn context_turns() -> usize {
    std::env::var("SHAKEDOWN_CONTEXT_TURNS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(CONTEXT_TURNS)
}

/// Trims a long-lived session's context before a turn: each tool result longer than
/// [`TRIM_MIN_CHARS`] in a turn older than the latest `keep` (a turn starts at a user message)
/// becomes a short placeholder saying how long it was. The agent's own messages, its tool calls
/// and its reasoning stay. The same messages and `keep` always give the same context, and each
/// model call's request is recorded as sent, so the record shows what the model saw. Gives how
/// many results it trimmed; `keep` 0 trims nothing.
pub(crate) fn trim_context(messages: &mut [Value], keep: usize) -> u32 {
    let turns: Vec<usize> = (messages.iter().enumerate())
        .filter(|(_, m)| m["role"] == "user")
        .map(|(i, _)| i)
        .collect();
    if keep == 0 || turns.len() <= keep {
        return 0;
    }
    let cutoff = turns[turns.len() - keep];
    let mut trimmed = 0;
    for m in &mut messages[..cutoff] {
        let long = (m["content"].as_str())
            .map(|c| c.chars().count())
            .filter(|n| *n > TRIM_MIN_CHARS && m["role"] == "tool");
        if let Some(n) = long {
            m["content"] = json!(format!(
                "[older tool result trimmed by Shakedown: {n} characters; call the tool again if you \
                 still need it]"
            ));
            trimmed += 1;
        }
    }
    trimmed
}

/// One model call's request: the conversation, the tools and the model, with the agent's
/// `reasoning` and `max_tokens` when it has them.
fn call_body(
    messages: &[Value],
    tools: &[Value],
    model: &str,
    reasoning: Option<&Value>,
    max_tokens: Option<u32>,
) -> Value {
    let mut body = json!({"messages": messages, "tools": tools, "model": model});
    if let Some(r) = reasoning {
        body["reasoning"] = r.clone();
    }
    if let Some(n) = max_tokens {
        body["max_tokens"] = json!(n);
    }
    body
}

/// What an agent's loop talks to: the gateway to its provider and model, and its tools.
struct Loop {
    state: Arc<AppState>,
    provider: Provider,
    model: String,
    gateway: Gateway,
    tools: Vec<Value>,
    branch: Arc<BranchCtx>,
    agent: String,
    /// The agent's `reasoning` setting, sent with every call when it has one.
    reasoning: Option<Value>,
    /// The agent's `max_tokens`, sent with every call when it has one.
    max_tokens: Option<u32>,
}

impl Loop {
    fn new(
        state: Arc<AppState>,
        branch: Arc<BranchCtx>,
        spec: &AgentSpec,
        providers: &ProviderConfig,
    ) -> Result<Loop, Fault> {
        let (provider, model) = provider_and_model(providers, spec)?;
        Ok(Loop {
            gateway: Gateway {
                url: gateway_url(&state, &branch.branch_id, &spec.id),
                token: branch.agent_token(&spec.id),
            },
            tools: openai_tools(&branch, &spec.id),
            model: model.to_string(),
            provider,
            state,
            branch,
            agent: spec.id.clone(),
            reasoning: spec.reasoning.clone(),
            max_tokens: spec.max_tokens,
        })
    }

    /// The request of the loop's next model call on `messages`.
    fn body(&self, messages: &[Value]) -> Value {
        let (reasoning, max_tokens) = (self.reasoning.as_ref(), self.max_tokens);
        call_body(messages, &self.tools, &self.model, reasoning, max_tokens)
    }

    /// Model calls, running the tools each one asks for, until the agent answers without a tool
    /// call or ends its part (`handoff`, `finish`); what it said last.
    async fn until_done(
        &self,
        messages: &mut Vec<Value>,
        totals: &mut Totals,
    ) -> Result<String, Fault> {
        let exec = ToolExec::new(self.state.store.clone());
        let base = self.state.config.retry_base;
        let mut said = String::new();
        for _ in 0..max_model_calls() {
            let v = model_call(&self.state.http, &self.gateway, &self.body(messages), base).await?;
            totals.add(&self.provider, &self.model, &v["usage"]);
            // The gateway priced the call into the run's estimate: stop once it is over.
            if let Some(why) = self.branch.budget.over() {
                return Err(Fault::Budget(why));
            }
            let msg = v["choices"][0]["message"].clone();
            if !msg.is_object() {
                return Err("model response has no message".into());
            }
            messages.push(msg.clone());
            let text = msg["content"].as_str().unwrap_or("").to_string();
            if !text.trim().is_empty() {
                said = text.clone();
            }
            let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
            if calls.is_empty() {
                return end_without_calls(&v).map(|()| text);
            }
            if run_calls(&exec, &self.branch, &self.agent, calls, messages).await? {
                return Ok(text);
            }
        }
        Err(Fault::CallLimit(said))
    }
}

impl OpenDriver {
    async fn run_inner(&self, run: AgentRun) -> AgentResult {
        let start = Instant::now();
        let mut totals = Totals::new();
        let outcome = self.drive(&run, &mut totals).await;
        let (error, failure) = match outcome.map_err(Fault::split) {
            Ok(()) => (None, None),
            Err((e, failure)) => (Some(e), failure),
        };
        totals.result(error, failure, start.elapsed().as_secs_f64())
    }

    async fn drive(&self, run: &AgentRun, totals: &mut Totals) -> Result<(), Fault> {
        let state = self.state.upgrade().ok_or("server is shutting down")?;
        // The caller's own keys are known here, so an agent may name one (`key:<name>`).
        let providers = keys::config_for(&state, &run.branch.caller).await;
        let lp = Loop::new(state, run.branch.clone(), &run.spec, &providers)?;
        let mut messages = initial_messages(run);
        lp.until_done(&mut messages, totals).await.map(drop)
    }
}

/// The agent's provider, and its model, else the provider's default; an error when the
/// provider is unknown or has no default.
fn provider_and_model<'a>(
    providers: &ProviderConfig,
    spec: &'a AgentSpec,
) -> Result<(Provider, &'a str), Fault> {
    // The gateway resolves the real provider with your keys; this one prices the calls.
    let provider = providers.resolve_pricing(spec.provider.as_deref())?;
    let named = spec.model.as_deref().filter(|m| !m.trim().is_empty());
    let model = named.or(provider.default_model()).ok_or_else(|| {
        Fault::Infra(format!(
            "agent {} names no model, and provider {} has no default one",
            spec.id, provider.id
        ))
    })?;
    Ok((provider, model))
}

/// The model answered with no tool calls: done, unless its output was cut off or it refused
/// (`content_filter`, with the category from Claude's `stop_details` when there is one).
fn end_without_calls(response: &Value) -> Result<(), Fault> {
    let choice = &response["choices"][0];
    match choice["finish_reason"].as_str() {
        Some("length") => Err(Fault::Model("model output was cut off".into())),
        Some("content_filter") => Err(Fault::Model(refusal(&choice["stop_details"]))),
        _ => Ok(()),
    }
}

fn refusal(details: &Value) -> String {
    match details["category"].as_str() {
        Some(category) => format!("the model refused (category {category})"),
        None => "the model refused".to_string(),
    }
}

/// Runs each tool call, appending the tool messages. `Ok(true)` when the agent handed off, or
/// finished a dispatch run.
async fn run_calls(
    exec: &ToolExec,
    branch: &Arc<BranchCtx>,
    agent: &str,
    calls: Vec<Value>,
    messages: &mut Vec<Value>,
) -> Result<bool, Fault> {
    for call in calls {
        let id = call["id"].as_str().unwrap_or("").to_string();
        let name = call["function"]["name"].as_str().unwrap_or("").to_string();
        let raw = call["function"]["arguments"].as_str().unwrap_or("");
        let Some(args) = parse_tool_args(raw) else {
            let content = json!({"error": format!(
                "arguments for {name} were not valid JSON (the output may have been cut off); the tool was not run")});
            messages.push(tool_message(&id, content.to_string()));
            continue;
        };
        let res = exec
            .run_outcome(branch, agent, &name, args, Some(id.clone()))
            .await;
        let content = match &res {
            Ok(v) => v.to_string(),
            Err(e) => json!({"error": e}).to_string(),
        };
        messages.push(tool_message(&id, content));
        if [HANDOFF, FINISH].contains(&name.as_str()) && res.is_ok() {
            return Ok(true);
        }
        // A world program stopped: the branch can't go on.
        if let Some(f) = branch.lifecycle.fatal() {
            return Err(Fault::Infra(f));
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convo(turns: usize, result_len: usize) -> Vec<Value> {
        let mut m = vec![json!({"role": "system", "content": "brief"})];
        for t in 0..turns {
            m.push(json!({"role": "user", "content": format!("turn {t}")}));
            m.push(json!({"role": "assistant", "content": format!("thinking {t}"), "reasoning": "r",
                          "tool_calls": [{"id": format!("c{t}"), "type": "function",
                                          "function": {"name": "ops_flights", "arguments": "{}"}}]}));
            m.push(json!({"role": "tool", "tool_call_id": format!("c{t}"), "content": "x".repeat(result_len)}));
        }
        m
    }

    #[test]
    fn the_call_limit_is_a_whole_number_above_zero_else_forty() {
        assert_eq!(call_limit(None), 40);
        assert_eq!(call_limit(Some(" 100 ")), 100);
        for bad in ["0", "-3", "many", ""] {
            assert_eq!(call_limit(Some(bad)), MAX_MODEL_CALLS, "{bad:?}");
        }
    }

    #[test]
    fn a_session_turn_at_the_call_limit_ends_with_a_note_not_a_failure() {
        let t = session_turn(Err(Fault::CallLimit("still checking".into())));
        assert!(t.ok && t.error.is_none());
        assert_eq!(
            (t.text.as_str(), t.note.as_deref()),
            ("still checking", Some(CALL_LIMIT_NOTE))
        );
        let (e, failure) = Fault::CallLimit(String::new()).split();
        assert!(
            e.contains("exceeded") && failure == Some(FailureKind::Model),
            "a one-shot run still fails"
        );
        let t = session_turn(Err(Fault::Model("bad".into())));
        assert!(!t.ok && t.note.is_none());
    }

    #[test]
    fn old_bulky_tool_results_are_trimmed_and_everything_else_kept() {
        let mut m = convo(6, 5_000);
        let before = m.clone();
        assert_eq!(
            trim_context(&mut m, 4),
            2,
            "turns 0 and 1 are older than the latest four"
        );
        for (i, (a, b)) in m.iter().zip(&before).enumerate() {
            let old_tool = b["role"] == "tool" && i < 7;
            match old_tool {
                true => {
                    let c = a["content"].as_str().unwrap();
                    assert!(
                        c.starts_with("[older tool result trimmed by Shakedown: 5000 characters"),
                        "{c}"
                    );
                    assert_eq!(
                        a["tool_call_id"], b["tool_call_id"],
                        "the call keeps its result's id"
                    );
                }
                false => assert_eq!(a, b, "message {i} is kept as it was"),
            }
        }
        // The same context again trims nothing more, and the result is the same.
        let again = m.clone();
        assert_eq!(trim_context(&mut m, 4), 0);
        assert_eq!(m, again);
    }

    #[test]
    fn short_results_recent_turns_and_a_zero_window_are_left_alone() {
        let mut short = convo(6, 100);
        assert_eq!(trim_context(&mut short, 4), 0);
        let mut recent = convo(4, 5_000);
        assert_eq!(trim_context(&mut recent, 4), 0);
        let mut off = convo(9, 5_000);
        assert_eq!(trim_context(&mut off, 0), 0);
    }

    #[test]
    fn a_call_carries_the_agents_reasoning_and_max_tokens_only_when_set() {
        let msgs = [json!({"role": "user", "content": "hi"})];
        let plain = call_body(&msgs, &[], "m", None, None);
        assert!(plain.get("reasoning").is_none() && plain.get("max_tokens").is_none());
        let set = call_body(
            &msgs,
            &[],
            "m",
            Some(&json!({"effort": "high"})),
            Some(16_000),
        );
        assert_eq!(set["reasoning"], json!({"effort": "high"}));
        assert_eq!(set["max_tokens"], json!(16_000));
        assert_eq!(set["model"], "m");
    }

    /// An open agent's end counts each call at the price the gateway charges it: on Claude, whose
    /// answers say no cost, the list price with cache reads at the cache rate.
    #[test]
    fn an_open_claude_agents_cost_prices_cache_reads_at_the_cache_rate() {
        let claude = ProviderConfig::default()
            .resolve(Some("anthropic"))
            .unwrap();
        let usage = crate::providers::canonical::cached_usage(12, 0, 30, 5);
        let mut totals = Totals::new();
        totals.add(&claude, "claude-sonnet-5-5", &usage);
        let r = totals.result(None, None, 1.0);
        let t = crate::prices::CallTokens {
            input: 12,
            cache_write: 0,
            cache_read: 30,
            output: 5,
        };
        let want = crate::prices::call_cost("anthropic", "claude-sonnet-5-5", &t);
        assert!(want.is_some(), "the table prices claude-sonnet-5-5");
        assert_eq!(r.cost_usd, want);
        assert_eq!(r.cost_source.as_deref(), Some("list_price_estimate"));
    }

    #[test]
    fn retries_wait_as_asked_else_twice_as_long_each_time() {
        let base = Duration::from_secs(1);
        let waits: Vec<u64> = (0..3)
            .map(|n| retry_wait(base, n, None).as_secs())
            .collect();
        assert_eq!(waits, [1, 2, 4]);
        assert_eq!(retry_wait(base, 2, Some(0)), Duration::ZERO);
        assert_eq!(retry_wait(base, 0, Some(7)).as_secs(), 7);
        assert_eq!(retry_wait(base, 0, Some(600)), MAX_RETRY_WAIT);
        assert_eq!(retry_wait(base, 30, None), MAX_RETRY_WAIT);
    }

    /// A rate limit gets retries enough to outlast a per-minute limit, bounded; an overload fewer.
    #[test]
    fn a_rate_limit_is_waited_out_for_a_minute_at_most() {
        let base = Duration::from_secs(1);
        let total = |status| -> u64 {
            let waits = (0..retries_for(status)).map(|n| retry_wait(base, n, None).as_secs());
            waits.sum()
        };
        assert_eq!(total(429), 61, "1 + 2 + 4 + 8 + 16 + 30");
        assert_eq!(total(503), 7);
        let said = tried(
            "model call failed (429): slow down",
            429,
            7,
            Duration::from_secs(61),
        );
        assert_eq!(
            said,
            "rate-limited by the model provider (7 tries over 61 s): model call failed (429): slow down"
        );
        let said = tried(
            "model call failed (503): busy",
            503,
            4,
            Duration::from_secs(7),
        );
        assert_eq!(said, "model call failed (503): busy (4 tries over 7 s)");
    }

    /// Arguments cut off mid-JSON (or not an object) are not run; none at all are `{}`.
    #[test]
    fn only_a_json_object_is_run_as_arguments() {
        assert_eq!(parse_tool_args(""), Some(json!({})));
        assert_eq!(
            parse_tool_args(r##"{"order_id": "#W1"}"##),
            Some(json!({"order_id": "#W1"}))
        );
        assert_eq!(parse_tool_args(r##"{"order_id": "#W00"##), None);
        assert_eq!(parse_tool_args("[1]"), None);
    }

    /// An answer without tool calls ends the agent well, unless its output was cut off or the
    /// model refused: both are the model's failures, not the infrastructure's.
    #[test]
    fn a_cut_off_or_refused_answer_is_a_model_failure() {
        let answer = |finish: &str, details: Value| {
            json!({"choices": [{"message": {"role": "assistant", "content": "x"},
                                "finish_reason": finish, "stop_details": details}]})
        };
        assert!(end_without_calls(&answer("stop", Value::Null)).is_ok());
        let model = |r: Result<(), Fault>| match r {
            Err(Fault::Model(e)) => e,
            _ => panic!("not a model failure"),
        };
        assert_eq!(
            model(end_without_calls(&answer("length", Value::Null))),
            "model output was cut off"
        );
        let refused = answer(
            "content_filter",
            json!({"type": "refusal", "category": "cyber"}),
        );
        assert_eq!(
            model(end_without_calls(&refused)),
            "the model refused (category cyber)"
        );
        assert_eq!(
            model(end_without_calls(&answer("content_filter", Value::Null))),
            "the model refused"
        );
    }

    #[test]
    fn failures_say_what_the_provider_said() {
        let claude = json!({"error": "upstream returned 529", "status": 529, "body":
            {"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}});
        let status = reqwest::StatusCode::from_u16(529).unwrap();
        assert_eq!(
            failure(status, &claude.to_string()),
            "model call failed (529): Overloaded"
        );
        let unknown = json!({"error": "unknown provider \"x\""}).to_string();
        let bad = reqwest::StatusCode::BAD_GATEWAY;
        assert_eq!(
            failure(bad, &unknown),
            "model call failed (502): unknown provider \"x\""
        );
        assert_eq!(
            failure(bad, "not json"),
            "model call failed (502): not json"
        );
    }
}
