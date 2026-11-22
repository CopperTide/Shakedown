//! `GET /worker`: the control box's end of a worker's link, one WebSocket per connection. The
//! first frame is `Hello`, with a ticket (answered `Start`, with the branch's worker token) or, on
//! a reconnect, that worker token. Each `Event` is appended once per `n` and acknowledged; a
//! resent one is acknowledged again. `Ended` records how the branch ended and revokes the token.
//! Pause, resume and stop pressed in the UI are relayed to the open connection.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    routing::get,
    Router,
};
use chrono::Utc;
use tokio::sync::mpsc::UnboundedReceiver;
use uuid::Uuid;

use super::{
    frames::{ToControl, ToWorker},
    links::Link,
};
use crate::{
    app::AppState,
    auth::{
        engine_check,
        run_tokens::{prefix, resolve_run_token, DEFAULT_TOKEN_SECS},
        Action, Caller, CallerKind, Decision, RunTokenKind, Target,
    },
    exec::{BranchRecords, StoreRecords},
    model::BranchStatus,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/worker", get(upgrade))
}

async fn upgrade(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| serve(state, socket))
}

/// How a connection ends: `Refused` with why (the worker should not come back with the same
/// hello), or closed (a reconnect may go on).
enum End {
    Refuse(String),
    Close,
}

fn refused(why: impl Into<String>) -> End {
    End::Refuse(why.into())
}

/// A connection past its hello: its branch's link, its generation, who the branch records for,
/// and the frames relayed to it.
struct Opened {
    link: Arc<Link>,
    generation: u64,
    caller: Arc<Caller>,
    relay: UnboundedReceiver<ToWorker>,
}

impl Opened {
    fn records(&self, state: &AppState) -> StoreRecords {
        let branch = self.link.plan.branch_id.clone();
        StoreRecords::new(state.store.clone(), self.caller.clone(), branch)
    }
}

async fn serve(state: Arc<AppState>, mut socket: WebSocket) {
    let opened = match hello(&state, &mut socket).await {
        Ok(o) => o,
        Err(end) => return finish(&mut socket, end).await,
    };
    let (link, generation) = (opened.link.clone(), opened.generation);
    let end = pump(&state, &mut socket, opened).await;
    link.detach(generation);
    finish(&mut socket, end).await;
}

/// Sends `Refused` when the connection was refused, then closes it.
async fn finish(socket: &mut WebSocket, end: End) {
    if let End::Refuse(why) = end {
        let _ = say(socket, &ToWorker::Refused { why }).await;
    }
    let _ = socket.send(Message::Close(None)).await;
}

async fn say(socket: &mut WebSocket, f: &ToWorker) -> Result<(), End> {
    let text = serde_json::to_string(f).map_err(|_| End::Close)?;
    let sent = socket.send(Message::Text(text.into())).await;
    sent.map_err(|_| End::Close)
}

/// The worker's next frame: closed when the connection is; refused when it is no frame.
async fn next_frame(socket: &mut WebSocket) -> Result<ToControl, End> {
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(t))) => {
                return serde_json::from_str(&t).map_err(|e| refused(format!("bad frame: {e}")))
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            Some(Ok(Message::Binary(_))) => return Err(refused("frames are text")),
            Some(Ok(Message::Close(_)) | Err(_)) | None => return Err(End::Close),
        }
    }
}

/// Reads the hello and opens the connection on its branch's link: `Start` is sent to a ticket's
/// worker first.
async fn hello(state: &AppState, socket: &mut WebSocket) -> Result<Opened, End> {
    let ToControl::Hello {
        ticket,
        resume_from,
    } = next_frame(socket).await?
    else {
        return Err(refused("the first frame must be hello"));
    };
    let (link, start) = match ticket.starts_with(prefix(RunTokenKind::Ticket)) {
        true => redeem(state, &ticket).await?,
        false => (rejoin(state, &ticket).await?, None),
    };
    let last = link.appended.lock().await.last_n;
    if resume_from > last {
        let why = format!("resume_from {resume_from} is past the last event appended ({last})");
        return Err(refused(why));
    }
    let caller = link.caller().ok_or(End::Close)?;
    let (generation, relay) = link.attach();
    if let Some(start) = start {
        say(socket, &start).await?;
    }
    Ok(Opened {
        link,
        generation,
        caller,
        relay,
    })
}

/// The link a ticket starts, and its `Start`: the plan, a new worker token and the gateway.
async fn redeem(state: &AppState, secret: &str) -> Result<(Arc<Link>, Option<ToWorker>), End> {
    let t = (state.links.tickets.redeem(secret))
        .ok_or_else(|| refused("unknown, used or expired ticket"))?;
    let link = (state.links.get(&t.branch_id))
        .ok_or_else(|| refused("no worker is expected for this branch"))?;
    let sandbox =
        Uuid::parse_str(&t.sandbox).map_err(|_| refused("the ticket's sandbox is no uuid"))?;
    if link.caller().is_some() {
        return Err(refused("this branch's worker has started already"));
    }
    let token = worker_token(state, &link, &t.caller, sandbox).await?;
    if !link.start_for(t.caller.clone()) {
        return Err(refused("this branch's worker has started already"));
    }
    let start = ToWorker::Start {
        plan: Box::new(link.plan.clone()),
        token,
        gateway: format!("{}/gateway", state.config.public_base()),
    };
    Ok((link, Some(start)))
}

/// A new worker token for `link`'s branch in `sandbox`, for `caller`: it lives as long as the
/// branch (revoked when it ends; a day at most, as an agent's).
async fn worker_token(
    state: &AppState,
    link: &Link,
    caller: &Caller,
    sandbox: Uuid,
) -> Result<String, End> {
    let plan = &link.plan;
    let expires = Utc::now() + chrono::Duration::seconds(DEFAULT_TOKEN_SECS as i64);
    let kind = RunTokenKind::Worker;
    let at = state.store.at(caller);
    let minted = at.mint_run_token(
        &plan.run_id,
        &plan.branch_id,
        kind,
        None,
        Some(sandbox),
        expires,
    );
    minted
        .await
        .map_err(|e| logged(&plan.branch_id, "minting its worker token", e))
}

fn logged(branch: &str, doing: &str, e: anyhow::Error) -> End {
    eprintln!("worker link {branch}: {doing}: {e:#}");
    End::Close
}

/// The link a worker token reconnects to: its own branch's, while this control box keeps it.
async fn rejoin(state: &AppState, secret: &str) -> Result<Arc<Link>, End> {
    let found = resolve_run_token(&state.store, secret).await;
    let found = found.map_err(|e| logged("?", "resolving a worker token", e))?;
    let unknown = || refused("unknown, revoked or expired worker token");
    let c = found.ok_or_else(unknown)?;
    let CallerKind::RunToken {
        run,
        branch,
        kind: RunTokenKind::Worker,
        ..
    } = &c.kind
    else {
        return Err(unknown());
    };
    let on = Target::Branch { run, branch };
    if engine_check(&c, &Action::WorkerLink, &on) != Decision::Allow {
        return Err(unknown());
    }
    let link = state.links.get(branch).filter(|l| l.caller().is_some());
    link.ok_or_else(|| refused("this control box keeps no link for that branch"))
}

/// What the connection waits on next: a frame relayed from the API, or one from the worker.
enum Got {
    Relay(Option<ToWorker>),
    Frame(Result<ToControl, End>),
}

/// Answers the worker's frames and relays the API's until the connection ends.
async fn pump(state: &AppState, socket: &mut WebSocket, mut o: Opened) -> End {
    loop {
        let got = tokio::select! {
            out = o.relay.recv() => Got::Relay(out),
            f = next_frame(socket) => Got::Frame(f),
        };
        let step = match got {
            Got::Relay(Some(f)) => say(socket, &f).await,
            // A newer connection took the relay over.
            Got::Relay(None) => Err(End::Close),
            Got::Frame(Ok(f)) => answer(state, socket, &o, f).await,
            Got::Frame(Err(end)) => Err(end),
        };
        if let Err(end) = step {
            return end;
        }
    }
}

async fn answer(
    state: &AppState,
    socket: &mut WebSocket,
    o: &Opened,
    f: ToControl,
) -> Result<(), End> {
    match f {
        ToControl::Hello { .. } => Err(refused("hello came twice")),
        ToControl::Ended { status, error } => Err(ended(state, o, status, error).await),
        event => {
            let ack = append(state, o, event).await?;
            say(socket, &ack).await
        }
    }
}

/// Appends event `n` when it is the next one, or acks it again when it was appended already.
async fn append(state: &AppState, o: &Opened, f: ToControl) -> Result<ToWorker, End> {
    let ToControl::Event {
        n,
        agent,
        kind,
        payload,
        overhead,
    } = f
    else {
        return Err(End::Close);
    };
    let mut done = o.link.appended.lock().await;
    if !o.link.is_current(o.generation) {
        return Err(End::Close);
    }
    if let Some(seq) = done.resent(n).map_err(refused)? {
        return Ok(ToWorker::Ack { n, seq });
    }
    let branch = &o.link.plan.branch_id;
    let records = o.records(state);
    let appended = records
        .append(agent.as_deref(), kind, payload, overhead)
        .await;
    let ev = appended.map_err(|e| logged(branch, "appending an event", e))?;
    done.push(n, ev.seq);
    Ok(ToWorker::Ack { n, seq: ev.seq })
}

/// Records how the branch ended, revokes its worker token and forgets its link: the connection
/// closes.
async fn ended(state: &AppState, o: &Opened, status: BranchStatus, error: Option<String>) -> End {
    if status == BranchStatus::Running {
        return refused("a branch does not end running");
    }
    let branch = &o.link.plan.branch_id;
    if let Err(e) = o.records(state).set_status(status, error).await {
        return logged(branch, "recording how it ended", e);
    }
    let at = state.store.at(&o.caller);
    if let Err(e) = at.revoke_run_tokens(branch, None).await {
        logged(branch, "revoking its worker token", e);
    }
    state.links.forget(branch);
    End::Close
}
