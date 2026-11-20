//! A request through shaked's proxy, run as a step: it waits while the branch is paused, holds the
//! branch gate, goes to the part serving its host, saves that part when it wrote, and is recorded
//! as a tool call named `http` owned by the part.

use bytes::Bytes;
use parts::proto::Effect;
use serde_json::{json, Value};

use super::{save_step, tool_call_payload, BranchCtx, CallFacts, ToolExec};
use crate::{
    model::EventKind,
    proxy::{
        forward::{forward, Forwarded, Sender},
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
    /// A plain-text answer of shaked's own.
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
    /// A request shaked refused (recorded with `why` as its error).
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

    /// Sends `call` to the part serving its host (the clock parts advanced first), unless shaked
    /// refuses it; saves the part when the request wrote.
    async fn exchange(
        &self,
        branch: &BranchCtx,
        agent: Option<&str>,
        call: &HttpCall,
    ) -> Exchanged {
        let mut facts = CallFacts::default();
        let admitted = match admit(branch, agent, call, &mut facts) {
            Ok(admitted) => admitted,
            Err((status, why)) => return Exchanged::refused(status, why, facts),
        };
        let clock = branch.clock.now_ms();
        facts.clock_ms = Some(clock);
        if let Err(e) = self.advance(branch, clock).await {
            return Exchanged::refused(503, e, facts);
        }
        let sender = Sender {
            agent,
            readonly: admitted.readonly,
        };
        let since = std::time::Instant::now();
        let forwarded = self.send(&admitted.route, call, sender).await;
        facts.duration_ms = Some(since.elapsed().as_secs_f64() * 1000.0);
        match forwarded {
            Ok(f) => answered(branch, &admitted, call, f, facts).await,
            Err((status, why)) => Exchanged::refused(status, why, facts),
        }
    }

    /// `call` sent to `route`'s upstream, its answer read whole within the executor's timeout;
    /// else 502 (no answer) or 504 (too slow) and why.
    async fn send(
        &self,
        route: &HostRoute,
        call: &HttpCall,
        sender: Sender<'_>,
    ) -> Result<Forwarded, (u16, String)> {
        let limit = self.http_timeout;
        match tokio::time::timeout(limit, forward(route, call, sender)).await {
            Ok(Ok(f)) => Ok(f),
            Ok(Err(e)) => Err((502, not_answered(route, &e))),
            Err(_) => Err((504, too_slow(route, limit))),
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

/// Why a request got no answer from the part serving its host.
fn not_answered(route: &HostRoute, e: &str) -> String {
    format!(
        "the {} part did not answer at {}: {e}",
        route.part, route.host
    )
}

/// Why a request answered 504: the part serving its host took longer than `limit`.
fn too_slow(route: &HostRoute, limit: std::time::Duration) -> String {
    let (part, host, secs) = (&route.part, &route.host, limit.as_secs_f64());
    format!("the {part} part did not answer at {host} within {secs} s")
}

/// A request shaked lets through: its host's route, whether it writes by the method rule, and
/// whether it goes marked read-only (a read-only agent's, to a route that honours read-only).
struct Admitted {
    route: HostRoute,
    write_by_rule: bool,
    readonly: bool,
}

/// The request admitted (the part is noted in `facts`); else the status and why shaked refuses
/// it: an unknown host, a branch that can't go on, a write of a read-only agent to a route that
/// does not honour read-only.
fn admit(
    branch: &BranchCtx,
    agent: Option<&str>,
    call: &HttpCall,
    facts: &mut CallFacts,
) -> Result<Admitted, (u16, String)> {
    let Some(route) = branch.world.hosts().route(&call.host).cloned() else {
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
    let read_only = agent.is_some_and(|a| branch.roster.read_only(a));
    if write_by_rule && read_only && !route.honours_readonly {
        return Err((403, READ_ONLY.to_string()));
    }
    let readonly = read_only && route.honours_readonly;
    Ok(Admitted {
        route,
        write_by_rule,
        readonly,
    })
}

/// The upstream's answer as a step: a read or a write of the host (the upstream's effect header
/// has the last word), the part saved when it wrote; a violation when a request marked read-only
/// wrote.
async fn answered(
    branch: &BranchCtx,
    admitted: &Admitted,
    call: &HttpCall,
    f: Forwarded,
    mut facts: CallFacts,
) -> Exchanged {
    let route = &admitted.route;
    let write = f
        .effect
        .map_or(admitted.write_by_rule, |e| e == Effect::Write);
    let key = vec![format!("http/{}", route.host)];
    match write {
        true => facts.writes = key,
        false => facts.reads = key,
    }
    save_step(branch, write.then_some(route.part.as_str()), &mut facts).await;
    let result = json!({"status": f.status, "headers": kept_headers(&f.headers),
                        "body": body_record(&f.body)});
    let outcome = match violation(branch, admitted, call, &f) {
        Some(why) => Err(why),
        None => Ok(result),
    };
    let answer = HttpAnswer {
        status: f.status,
        headers: f.headers,
        body: f.body,
    };
    Exchanged {
        answer,
        outcome,
        facts,
    }
}

/// A request marked read-only that the part says wrote (it should have refused it): why, noted
/// on the branch, which then fails.
fn violation(
    branch: &BranchCtx,
    admitted: &Admitted,
    call: &HttpCall,
    f: &Forwarded,
) -> Option<String> {
    if !admitted.readonly || f.effect != Some(Effect::Write) {
        return None;
    }
    let why = format!(
        "a read-only agent's request wrote: the {} part answered X-Shakedown-Effect: write to {} {}",
        admitted.route.part,
        call.method,
        call.url()
    );
    branch.lifecycle.set_violation(&why);
    Some(why)
}
