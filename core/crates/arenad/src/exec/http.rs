//! A request through arenad's proxy, run as a step: it waits while the branch is paused, holds the
//! branch gate, goes to the part serving its host, saves that part when it wrote, and is recorded
//! as a tool call named `http` owned by the part.

use bytes::Bytes;
use parts::proto::Effect;
use serde_json::{json, Value};

use super::{save_step, tool_call_payload, BranchCtx, CallFacts, ToolExec};
use crate::{
    model::EventKind,
    proxy::{
        forward::{forward, Forwarded},
        record::{body_record, kept_headers},
    },
    world::{HostRoute, HTTP_TOOL},
};

/// The answer to a request on a branch that isn't running (not recorded).
pub const NOT_RUNNING: &str = "the run is not running";
/// `args.attributed` of a driver's request recorded as the latest model caller's.
pub const ATTRIBUTED: &str = "latest model call";
const READ_ONLY: &str =
    "Write requests are not available to this agent: its team made it read-only";

/// A request an agent's client sent through the proxy.
#[derive(Debug, Clone)]
pub struct HttpCall {
    /// `http`, or `https` for a request inside a `CONNECT` tunnel.
    pub scheme: &'static str,
    pub method: String,
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
}

impl HttpCall {
    /// The URL the client asked for (`http://toy.test/hello`), the port only when it isn't the
    /// scheme's (always for a `CONNECT`: `https://toy.test:443`).
    pub fn url(&self) -> String {
        let (scheme, host, pq) = (self.scheme, &self.host, &self.path_and_query);
        if self.method == "CONNECT" {
            return format!("{scheme}://{host}:{}", self.port);
        }
        match (scheme, self.port) {
            ("http", 80) | ("https", 443) => format!("{scheme}://{host}{pq}"),
            (_, port) => format!("{scheme}://{host}:{port}{pq}"),
        }
    }

    /// The path, without the query.
    fn path(&self) -> &str {
        let pq = self.path_and_query.as_str();
        pq.split_once('?').map_or(pq, |(path, _)| path)
    }

    /// The step's `args`: method, URL, headers (no secrets) and body.
    fn args(&self) -> Value {
        json!({"method": self.method, "url": self.url(),
               "headers": kept_headers(&self.headers), "body": body_record(&self.body)})
    }
}

/// What the agent's client gets back.
#[derive(Debug, Clone)]
pub struct HttpAnswer {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
}

impl HttpAnswer {
    /// A plain-text answer of arenad's own.
    pub fn text(status: u16, text: &str) -> HttpAnswer {
        let plain = (
            "content-type".to_string(),
            "text/plain; charset=utf-8".to_string(),
        );
        HttpAnswer {
            status,
            headers: vec![plain],
            body: Bytes::from(text.to_string()),
        }
    }
}

/// A step's answer, its outcome as recorded, and what else it left to record.
struct Exchanged {
    answer: HttpAnswer,
    outcome: Result<Value, String>,
    facts: CallFacts,
}

impl Exchanged {
    /// A request arenad refused (recorded with `why` as its error).
    fn refused(status: u16, why: String, facts: CallFacts) -> Exchanged {
        Exchanged {
            answer: HttpAnswer::text(status, &why),
            outcome: Err(why),
            facts,
        }
    }
}

impl ToolExec {
    /// Runs `call` as a step of `agent` on `branch`, and records it. A request with no agent (the
    /// driver's ticket) is the agent's whose model call came last, when one did.
    pub async fn http(
        &self,
        branch: &BranchCtx,
        agent: Option<&str>,
        call: HttpCall,
    ) -> HttpAnswer {
        self.http_step(branch, agent, call, None).await
    }

    /// Records `call` as refused with `status` and `why`, in order with the branch's other steps.
    pub async fn http_refusal(
        &self,
        branch: &BranchCtx,
        agent: Option<&str>,
        call: HttpCall,
        (status, why): (u16, &str),
    ) -> HttpAnswer {
        self.http_step(branch, agent, call, Some((status, why)))
            .await
    }

    async fn http_step(
        &self,
        branch: &BranchCtx,
        agent: Option<&str>,
        call: HttpCall,
        refused: Option<(u16, &str)>,
    ) -> HttpAnswer {
        if branch.flow.go().await.is_err() {
            return HttpAnswer::text(409, NOT_RUNNING);
        }
        let _gate = branch.gate_timed().await;
        if branch.lifecycle.is_closed() {
            return HttpAnswer::text(409, NOT_RUNNING);
        }
        let attributed = agent.map_or_else(|| branch.roster.last_model_caller(), |_| None);
        let agent = agent.or(attributed.as_deref());
        let done = match refused {
            Some((status, why)) => {
                Exchanged::refused(status, why.to_string(), CallFacts::default())
            }
            None => self.exchange(branch, agent, &call).await,
        };
        let mut args = call.args();
        if attributed.is_some() {
            args["attributed"] = json!(ATTRIBUTED);
        }
        self.record_http(branch, agent, args, done.outcome, done.facts)
            .await;
        done.answer
    }

    /// Sends `call` to the part serving its host (the clock parts advanced first), unless arenad
    /// refuses it; saves the part when the request wrote.
    async fn exchange(
        &self,
        branch: &BranchCtx,
        agent: Option<&str>,
        call: &HttpCall,
    ) -> Exchanged {
        let mut facts = CallFacts::default();
        let (route, write_by_rule) = match admit(branch, agent, call, &mut facts) {
            Ok(admitted) => admitted,
            Err((status, why)) => return Exchanged::refused(status, why, facts),
        };
        let clock = branch.clock.now_ms();
        facts.clock_ms = Some(clock);
        if let Err(e) = self.advance(branch, clock).await {
            return Exchanged::refused(503, e, facts);
        }
        let since = std::time::Instant::now();
        let forwarded = forward(route, call).await;
        facts.duration_ms = Some(since.elapsed().as_secs_f64() * 1000.0);
        match forwarded {
            Ok(f) => answered(branch, route, f, write_by_rule, facts).await,
            Err(e) => {
                let why = format!(
                    "the {} part did not answer at {}: {e}",
                    route.part, route.host
                );
                Exchanged::refused(502, why, facts)
            }
        }
    }

    /// Records the step (a view after a write, as for a tool call); failures are logged.
    async fn record_http(
        &self,
        branch: &BranchCtx,
        agent: Option<&str>,
        args: Value,
        outcome: Result<Value, String>,
        facts: CallFacts,
    ) {
        let wrote = facts.wrote_world();
        let payload = tool_call_payload(HTTP_TOOL, args, &outcome, None, facts);
        match self
            .record_seq(branch, agent, EventKind::ToolCall, &payload)
            .await
        {
            Ok(seq) if wrote => self.record_view(branch, seq).await,
            Ok(_) => {}
            Err(e) => eprintln!("proxy: recording a request on {}: {e}", branch.branch_id),
        }
    }
}

/// The route of `call`'s host and whether the request writes by the method rule (the part is
/// noted in `facts`); else the status and why arenad refuses it: an unknown host, a branch that
/// can't go on, a write of a read-only agent.
fn admit<'w>(
    branch: &'w BranchCtx,
    agent: Option<&str>,
    call: &HttpCall,
    facts: &mut CallFacts,
) -> Result<(&'w HostRoute, bool), (u16, String)> {
    let Some(route) = branch.world.hosts().route(&call.host) else {
        return Err((
            403,
            format!("the host {} is not part of this world", call.host),
        ));
    };
    facts.part = Some(route.part.clone());
    if let Some(fatal) = branch.lifecycle.fatal() {
        return Err((503, fatal));
    }
    let write_by_rule = !route.is_read(&call.method, call.path());
    if write_by_rule && agent.is_some_and(|a| branch.roster.read_only(a)) {
        return Err((403, READ_ONLY.to_string()));
    }
    Ok((route, write_by_rule))
}

/// The upstream's answer as a step: a read or a write of the host (the upstream's effect header
/// has the last word), the part saved when it wrote.
async fn answered(
    branch: &BranchCtx,
    route: &HostRoute,
    f: Forwarded,
    write_by_rule: bool,
    mut facts: CallFacts,
) -> Exchanged {
    let write = f.effect.map_or(write_by_rule, |e| e == Effect::Write);
    let key = vec![format!("http/{}", route.host)];
    match write {
        true => facts.writes = key,
        false => facts.reads = key,
    }
    let written = write.then_some(route.part.as_str());
    save_step(branch, written, &mut facts).await;
    let result = json!({"status": f.status, "headers": kept_headers(&f.headers),
                        "body": body_record(&f.body)});
    let answer = HttpAnswer {
        status: f.status,
        headers: f.headers,
        body: f.body,
    };
    Exchanged {
        answer,
        outcome: Ok(result),
        facts,
    }
}
