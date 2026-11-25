//! `GET /worker`: the control box's end of a worker's link, one WebSocket per connection. The
//! first frame is `Hello`, within [`Config::link_hello_timeout`](crate::app::Config), with a
//! ticket (answered `Start`, with the branch's worker token) or, on a reconnect, that worker
//! token; a link this control box lost (it restarted) is rebuilt from the token and the store.
//! Each `Event` is appended once per `n` (the store keeps `n`) and acknowledged; a resent one is
//! acknowledged again with the seq it got. `Ended` records how the branch ended and revokes the
//! token. Pause, resume and stop pressed in the UI are relayed to the open connection, which is
//! pinged and closed once it stays silent for `link_idle_timeout`.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        FromRequestParts, Request, State,
    },
    http::request::Parts,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use chrono::Utc;
use tokio::{sync::mpsc::UnboundedReceiver, time::Instant};

use super::{
    frames::{ToControl, ToWorker, REFUSED_ENDED, REFUSED_TOKEN},
    holds,
    links::{frame_of, HoldState, Link},
    tickets::Taken,
};
use crate::{
    app::AppState,
    auth::{
        run_tokens::{self, prefix, DEFAULT_TOKEN_SECS},
        Action, Caller, CallerKind, RunTokenKind,
    },
    model::{BranchStatus, Hold},
    runner::BranchPlan,
    store::Linked,
};

/// The largest frame (and message) a worker may send: an event as large as the store is asked
/// to keep. A model call's event holds its request and answer, so this is more than an API
/// body's 2 MiB.
pub const MAX_LINK_FRAME: usize = 8 * 1024 * 1024;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/worker", get(upgrade))
}

async fn upgrade(State(state): State<Arc<AppState>>, req: Request) -> Response {
    let (mut parts, _) = req.into_parts();
    let ws = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(ws) => ws,
        Err(r) => return r.into_response(),
    };
    let ws = ws
        .max_message_size(MAX_LINK_FRAME)
        .max_frame_size(MAX_LINK_FRAME);
    ws.on_upgrade(move |socket| serve(state, parts, socket))
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
struct Joined {
    link: Arc<Link>,
    generation: u64,
    caller: Arc<Caller>,
    relay: UnboundedReceiver<ToWorker>,
}

async fn serve(state: Arc<AppState>, parts: Parts, mut socket: WebSocket) {
    let within = state.config.link_hello_timeout;
    let first = tokio::time::timeout(within, next_frame(&mut socket)).await;
    let first = first.unwrap_or_else(|_| Err(refused("no hello in time")));
    let joined = match first {
        Ok(f) => hello(&state, &parts, &mut socket, f).await,
        Err(end) => Err(end),
    };
    let joined = match joined {
        Ok(j) => j,
        Err(end) => return finish(&mut socket, end).await,
    };
    let (link, generation) = (joined.link.clone(), joined.generation);
    let end = pump(&state, &mut socket, joined).await;
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

/// What the worker sent next: a frame, or None for a ping or pong (it is still there). Closed
/// when the connection is; refused when it sent no frame.
async fn heard(socket: &mut WebSocket) -> Result<Option<ToControl>, End> {
    match socket.recv().await {
        Some(Ok(Message::Text(t))) => serde_json::from_str(&t)
            .map(Some)
            .map_err(|e| refused(format!("bad frame: {e}"))),
        Some(Ok(Message::Ping(_) | Message::Pong(_))) => Ok(None),
        Some(Ok(Message::Binary(_))) => Err(refused("frames are text")),
        Some(Ok(Message::Close(_)) | Err(_)) | None => Err(End::Close),
    }
}

/// The worker's next frame, past pings and pongs.
async fn next_frame(socket: &mut WebSocket) -> Result<ToControl, End> {
    loop {
        if let Some(f) = heard(socket).await? {
            return Ok(f);
        }
    }
}

/// Opens the connection whose first frame is `first` on its branch's link: `Start` is sent to a
/// ticket's worker first.
async fn hello(
    state: &Arc<AppState>,
    parts: &Parts,
    socket: &mut WebSocket,
    first: ToControl,
) -> Result<Joined, End> {
    let ToControl::Hello {
        ticket,
        resume_from,
    } = first
    else {
        return Err(refused("the first frame must be hello"));
    };
    let (link, start) = match ticket.starts_with(prefix(RunTokenKind::Ticket)) {
        true => redeem(state, &ticket, resume_from).await?,
        false => (rejoin(state, parts, &ticket, resume_from).await?, None),
    };
    let caller = link.caller().ok_or(End::Close)?;
    let (generation, relay) = link.attach();
    if let Some(start) = start {
        say(socket, &start).await?;
    }
    tell_hold(socket, &link).await?;
    Ok(Joined {
        link,
        generation,
        caller,
        relay,
    })
}

/// Tells a worker that (re)joins what its branch is held to, if anything.
async fn tell_hold(socket: &mut WebSocket, link: &Link) -> Result<(), End> {
    match frame_of(link.hold(), &link.limit_reason()) {
        Some(f) => say(socket, &f).await,
        None => Ok(()),
    }
}

/// Refuses a `resume_from` past the last event `link` has.
async fn check_resume(link: &Link, resume_from: u64) -> Result<(), End> {
    let last = link.appended.lock().await.last_n;
    if resume_from > last {
        let why = format!("resume_from {resume_from} is past the last event appended ({last})");
        return Err(refused(why));
    }
    Ok(())
}

/// The link a ticket starts, and its `Start`: the plan, a new worker token and the gateway.
async fn redeem(
    state: &AppState,
    secret: &str,
    resume_from: u64,
) -> Result<(Arc<Link>, Option<ToWorker>), End> {
    if resume_from != 0 {
        return Err(refused("a ticket's worker resumes from 0"));
    }
    let (taken, link, plan) = take_ticket(state, secret)?;
    let caller = taken.ticket.caller.clone();
    let token = claimed_token(state, &link, secret, taken).await?;
    let keys = crate::providers::keys::named_in(state, &caller, &plan.team).await;
    let start = ToWorker::Start {
        plan: Box::new(plan),
        token,
        gateway: format!("{}/gateway", state.config.public_base()),
        keys,
    };
    Ok((link, Some(start)))
}

/// The ticket `secret`, used up only when its branch's link is waiting for its worker: the
/// ticket, the link and its plan.
fn take_ticket(state: &AppState, secret: &str) -> Result<(Taken, Arc<Link>, BranchPlan), End> {
    let waiting = |branch: &str| state.links.get(branch).filter(|l| l.caller().is_none());
    let taken = (state.links.tickets).take_if(secret, |t| waiting(&t.branch_id).is_some());
    let taken = taken.ok_or_else(|| refused("unknown, used or expired ticket"))?;
    let link = waiting(&taken.ticket.branch_id);
    let link = link.ok_or_else(|| refused("this branch's worker has started"))?;
    let plan = link.plan.clone();
    let plan = plan.ok_or_else(|| refused("no plan for this branch"))?;
    Ok((taken, link, plan))
}

/// Claims `link`'s branch for the ticket's caller, then makes its worker token: of two tickets
/// for one branch only one makes a token. When the token could not be made, the claim is let go
/// and the ticket given back.
async fn claimed_token(
    state: &AppState,
    link: &Link,
    secret: &str,
    taken: Taken,
) -> Result<String, End> {
    let t = &taken.ticket;
    if !link.start_for(t.caller.clone()) {
        return Err(refused("this branch's worker has started"));
    }
    let made = worker_token(state, link, &t.caller, t.sandbox).await;
    if made.is_err() {
        link.release();
        state.links.tickets.give_back(secret, taken);
    }
    made
}

/// A new worker token for `link`'s branch in `sandbox`, for `caller`: it lives as long as the
/// branch (revoked when it ends; a day at most, as an agent's).
async fn worker_token(
    state: &AppState,
    link: &Link,
    caller: &Caller,
    sandbox: uuid::Uuid,
) -> Result<String, End> {
    let expires = Utc::now() + chrono::Duration::seconds(DEFAULT_TOKEN_SECS as i64);
    let kind = RunTokenKind::Worker;
    let at = state.store.at(caller);
    let (run, branch) = (&link.run_id, &link.branch_id);
    let minted = at.mint_run_token(run, branch, kind, None, Some(sandbox), expires);
    let token = minted.await;
    token.map_err(|e| logged(branch, "minting its worker token", e))
}

fn logged(branch: &str, doing: &str, e: anyhow::Error) -> End {
    eprintln!("worker link {branch}: {doing}: {e:#}");
    End::Close
}

/// The worker token `secret` names, when it is one for a branch's link: its caller.
async fn worker_caller(state: &AppState, secret: &str) -> Result<Caller, End> {
    let found = run_tokens::worker_caller(&state.store, secret, Action::WorkerLink, None).await;
    let found = found.map_err(|e| logged("?", "resolving a worker token", e))?;
    found.ok_or_else(|| refused(REFUSED_TOKEN))
}

/// The link a worker token reconnects to: its own branch's, rebuilt when this control box lost
/// it, while the branch is running. A rebuilt link records for whom the layer says the token
/// acts (the branch's owner), and goes on from the last `n` the store has.
async fn rejoin(
    state: &Arc<AppState>,
    parts: &Parts,
    secret: &str,
    resume_from: u64,
) -> Result<Arc<Link>, End> {
    let c = worker_caller(state, secret).await?;
    let CallerKind::RunToken { run, branch, .. } = c.kind.clone() else {
        return Err(End::Close);
    };
    let link = match state.links.get(&branch) {
        Some(link) if link.caller().is_none() => {
            return Err(refused("this branch's worker has not started"))
        }
        Some(link) => link,
        None => rebuilt(state, parts, c, &run, &branch).await?,
    };
    let caller = link.caller().ok_or(End::Close)?;
    still_running(state, &caller, &branch).await?;
    check_resume(&link, resume_from).await?;
    Ok(link)
}

/// Refuses a branch that is not running (any more): its worker has nothing to append.
async fn still_running(state: &AppState, caller: &Caller, branch: &str) -> Result<(), End> {
    let found = state.store.at(caller).find_branch(branch).await;
    let found = found.map_err(|e| logged(branch, "reading its status", e))?;
    match found {
        Some(b) if b.status == BranchStatus::Running => Ok(()),
        Some(b) => Err(refused(format!("{REFUSED_ENDED} {}", b.status.as_str()))),
        None => Err(refused("no such branch")),
    }
}

async fn rebuilt(
    state: &Arc<AppState>,
    parts: &Parts,
    c: Caller,
    run: &str,
    branch: &str,
) -> Result<Arc<Link>, End> {
    let layer = state.store.layer();
    let acting = layer.authenticate(parts, Some(c)).await;
    let acting = Arc::new(acting.map_err(|r| refused(r.message))?);
    let lock = state.store.branch_lock(branch);
    let locked = lock.lock().await;
    let (last, held) = on_record(state, &acting, branch).await?;
    let stopping = held.hold.ending();
    let link = state.links.rejoined((run, branch), acting, last, held);
    drop(locked);
    // A restart lost the grace of a branch the control box was stopping: it is armed again.
    if let Some(status) = stopping {
        holds::arm(state, branch, status, &link.limit_reason());
    }
    Ok(link)
}

/// What the store kept of `branch` for its rebuilt link: its worker's last `n`, and what it is
/// held to. A limit recorded reached whose hold was lost is put on that hold again.
async fn on_record(
    state: &AppState,
    acting: &Caller,
    branch: &str,
) -> Result<(u64, HoldState), End> {
    let at = state.store.at(acting);
    let last = at.last_link_n(branch).await;
    let last = last.map_err(|e| logged(branch, "reading its last event", e))?;
    let kept = at.controlled(branch).await;
    let kept = kept.map_err(|e| logged(branch, "reading what it is held to", e))?;
    let mut kept = kept.ok_or_else(|| refused("no such branch"))?;
    if kept.refused && kept.hold == Hold::Running {
        let repaired = at.set_hold(branch, Hold::OVER_BUDGET).await;
        repaired.map_err(|e| logged(branch, "holding it for its limit", e))?;
        kept.hold = Hold::OVER_BUDGET;
    }
    Ok((last, HoldState::of_record(&kept)))
}

/// What the connection waits on next: a frame relayed from the API, what the worker sent, or
/// the keepalive tick.
enum Got {
    Relay(Option<ToWorker>),
    Heard(Result<Option<ToControl>, End>),
    Tick,
}

/// When the worker was last heard from, and how long it may stay silent.
struct Keepalive {
    idle: std::time::Duration,
    last_heard: Instant,
}

/// Answers the worker's frames and relays the API's until the connection ends: it is pinged
/// every third of the idle timeout and closed once it was silent for all of it.
async fn pump(state: &AppState, socket: &mut WebSocket, mut o: Joined) -> End {
    let idle = state.config.link_idle_timeout;
    let every = (idle / 3).max(std::time::Duration::from_millis(10));
    let mut tick = tokio::time::interval(every);
    let mut keep = Keepalive {
        idle,
        last_heard: Instant::now(),
    };
    loop {
        let got = tokio::select! {
            out = o.relay.recv() => Got::Relay(out),
            h = heard(socket) => Got::Heard(h),
            _ = tick.tick() => Got::Tick,
        };
        if let Err(end) = step(state, socket, &o, got, &mut keep).await {
            return end;
        }
    }
}

async fn step(
    state: &AppState,
    socket: &mut WebSocket,
    o: &Joined,
    got: Got,
    keep: &mut Keepalive,
) -> Result<(), End> {
    match got {
        Got::Relay(Some(f)) => say(socket, &f).await,
        // A newer connection took the relay over.
        Got::Relay(None) => Err(End::Close),
        Got::Heard(h) => {
            keep.last_heard = Instant::now();
            match h? {
                Some(f) => answer(state, socket, o, f).await,
                None => Ok(()),
            }
        }
        Got::Tick if keep.last_heard.elapsed() >= keep.idle => Err(End::Close),
        Got::Tick => {
            let ping = socket.send(Message::Ping(Default::default())).await;
            ping.map_err(|_| End::Close)
        }
    }
}

async fn answer(
    state: &AppState,
    socket: &mut WebSocket,
    o: &Joined,
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

/// The event an `Event` frame carries.
fn linked(f: ToControl) -> Option<Linked> {
    let ToControl::Event {
        n,
        agent,
        kind,
        payload,
        overhead,
    } = f
    else {
        return None;
    };
    Some(Linked {
        n,
        agent,
        kind,
        payload,
        overhead,
    })
}

/// Appends event `n` when it is the next one, or acks it again when it was appended already
/// (known here, or found in the store).
async fn append(state: &AppState, o: &Joined, f: ToControl) -> Result<ToWorker, End> {
    let ev = linked(f).ok_or(End::Close)?;
    let n = ev.n;
    let mut done = o.link.appended.lock().await;
    if !o.link.is_current(o.generation) {
        return Err(End::Close);
    }
    if let Some(seq) = done.resent(n).map_err(refused)? {
        return Ok(ToWorker::Ack { n, seq });
    }
    let branch = &o.link.branch_id;
    let appended = state.store.at(&o.caller).append_linked(branch, ev).await;
    let ev = appended.map_err(|e| logged(branch, "appending an event", e))?;
    let ev = ev.ok_or_else(|| refused(format!("{REFUSED_ENDED}: it takes no more events")))?;
    done.push(n, ev.seq);
    Ok(ToWorker::Ack { n, seq: ev.seq })
}

/// Records how the branch ended, revokes its worker token and forgets its link: the connection
/// closes. A branch the control box was stopping ends as its hold says (`stopped`, or
/// `budget_exceeded`), whatever the worker reports; one it ended already is not changed. The hold
/// is read, and the end recorded, under the branch's lock.
async fn ended(state: &AppState, o: &Joined, status: BranchStatus, error: Option<String>) -> End {
    if status == BranchStatus::Running {
        return refused("a branch does not end running");
    }
    let branch = &o.link.branch_id;
    let lock = state.store.branch_lock(branch);
    let _locked = lock.lock().await;
    let hold = state.store.at(&o.caller).hold(branch).await;
    let hold = match hold {
        Ok(h) => h,
        Err(e) => return logged(branch, "reading its hold", e),
    };
    let (status, error) = match hold.ending() {
        Some(held) if held != status => (held, holds::stop_error(held, &o.link.limit_reason())),
        _ => (status, error),
    };
    let end = holds::end_locked(state, branch, (status, error), crate::jobs::EndIf::Running);
    if let Err(e) = end.await {
        return logged(branch, "recording how it ended", e);
    }
    End::Close
}
