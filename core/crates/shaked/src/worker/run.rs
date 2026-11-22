//! `shaked worker --link <ws(s)-url> --ticket-file <path>`: the engine in a sandbox. It waits for
//! the ticket file (the sandbox starts the worker before it is handed out), reads the ticket and
//! deletes the file, then connects out to the control box's `/worker`.
//!
//! **The ticket file.** Whoever hands the ticket over writes it to a temporary file in the same
//! folder and renames it to `--ticket-file`, the ticket ending with a newline. The worker reads
//! the file only once it ends with a newline (a file still being written is left alone), trims it
//! and deletes it. `Start` brings the branch's
//! plan and worker token; the worker stands the world up and drives the agents, recording every
//! event through the link ([`LinkRecords`]). It holds no database.
//!
//! The agents reach the worker's MCP endpoint, gateway and proxy on `127.0.0.1` only. The gateway
//! sends every model call on to `Start.gateway` with the worker token
//! ([`crate::gateway::ForwardTo`]): the control box adds the run's key and records the call. CLI
//! agents get that gateway as their API's base URL, their agent token as the key.
//!
//! A dropped link is tried again at once, then every [`Patience::retry`], with the worker token
//! and the last `n` acknowledged; every event not acknowledged is sent again, in order. The link
//! counts as down from the drop until the control box shows it works again on the new connection:
//! an ack, a pause, resume or stop, or a ping while nothing waits on it (no event unacknowledged,
//! no end unconfirmed). A connection that is accepted and then closed, or that pings but never
//! acknowledges, does not reset it. After [`Patience::give_up`] down the worker stops the world and exits 1 (the
//! control box ends the branch). `SHAKEDOWN_WORKER_RETRY_MS` and `SHAKEDOWN_WORKER_GIVE_UP_MS`
//! override both (tests).
//!
//! The worker's shaked runs on [`WorkerLayer`]: no vault, no keys from the environment.

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
    time::Instant,
};
use tokio_tungstenite::{
    tungstenite::{self, Message},
    MaybeTlsStream, WebSocketStream,
};

use super::{
    frames::{ToControl, ToWorker, GIVE_UP, REFUSED_ENDED, REFUSED_TOKEN},
    layer::WorkerLayer,
    link_records::{LinkRecords, Outbox, Request},
};
use crate::{
    app::{AppState, Config},
    exec::BranchCtx,
    gateway::ForwardTo,
    providers::{ProviderConfig, Secret},
    runner::{self, BranchPlan, StartPoint},
    store::Store,
};

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// How long a connection attempt may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the control box may take to answer a ticket's hello with `Start`.
const START_TIMEOUT: Duration = Duration::from_secs(60);
/// How often the ticket file is looked for.
const TICKET_POLL: Duration = Duration::from_millis(200);

/// How often a lost link is tried again, and for how long.
#[derive(Debug, Clone, Copy)]
pub struct Patience {
    pub retry: Duration,
    pub give_up: Duration,
}

impl Default for Patience {
    /// Every 2 s, for [`GIVE_UP`].
    fn default() -> Patience {
        Patience {
            retry: Duration::from_secs(2),
            give_up: GIVE_UP,
        }
    }
}

impl Patience {
    /// The default, with `SHAKEDOWN_WORKER_RETRY_MS` and `SHAKEDOWN_WORKER_GIVE_UP_MS` when set.
    pub fn from_env() -> Patience {
        let ms = |k: &str| {
            let v = parts::settings::var(k).ok()?;
            v.trim().parse().ok().map(Duration::from_millis)
        };
        let d = Patience::default();
        Patience {
            retry: ms("SHAKEDOWN_WORKER_RETRY_MS").unwrap_or(d.retry),
            give_up: ms("SHAKEDOWN_WORKER_GIVE_UP_MS").unwrap_or(d.give_up),
        }
    }
}

/// `shaked worker`'s arguments.
#[derive(Debug, PartialEq, Eq)]
struct WorkerArgs {
    link: String,
    ticket_file: PathBuf,
}

const USAGE: &str = "usage: shaked worker --link <ws(s)://host/worker> --ticket-file <path>";

impl WorkerArgs {
    fn parse(args: &[String]) -> Result<WorkerArgs> {
        let (mut link, mut ticket_file) = (None, None);
        let mut it = args.iter();
        while let Some(flag) = it.next() {
            let value = it
                .next()
                .with_context(|| format!("{flag} needs a value; {USAGE}"))?;
            match flag.as_str() {
                "--link" => link = Some(value.clone()),
                "--ticket-file" => ticket_file = Some(PathBuf::from(value)),
                other => bail!("unknown option {other}; {USAGE}"),
            }
        }
        Ok(WorkerArgs {
            link: link.context(USAGE)?,
            ticket_file: ticket_file.context(USAGE)?,
        })
    }
}

/// Runs `shaked worker` with `args` (after `worker`) to the branch's end: an error (exit 1) when
/// the branch could not start or its link was lost for good.
pub async fn command(args: &[String]) -> Result<()> {
    let args = WorkerArgs::parse(args)?;
    let ticket = take_ticket(&args.ticket_file).await?;
    let opened = open(&args.link, &ticket, Patience::from_env()).await?;
    drive(opened).await
}

/// The ticket in `path`, once the file is there and ends with a newline; the file is deleted.
async fn take_ticket(path: &std::path::Path) -> Result<String> {
    loop {
        match tokio::fs::read_to_string(path).await {
            Ok(t) if t.ends_with('\n') && !t.trim().is_empty() => {
                tokio::fs::remove_file(path).await?;
                return Ok(t.trim().to_string());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        }
        tokio::time::sleep(TICKET_POLL).await;
    }
}

/// A connection to `url` that said hello with `hello` (a ticket or the worker token).
async fn connect(url: &str, hello: &str, resume_from: u64) -> Result<Ws> {
    let connected = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url));
    let (mut ws, _) = connected.await.context("connecting timed out")??;
    let ticket = hello.to_string();
    let f = ToControl::Hello {
        ticket,
        resume_from,
    };
    ws.send(Message::text(serde_json::to_string(&f)?)).await?;
    Ok(ws)
}

/// The control box's first answer to a ticket: its `Start`.
async fn start_of(ws: &mut Ws) -> Result<ToWorker> {
    loop {
        let next = tokio::time::timeout(START_TIMEOUT, ws.next()).await;
        let text = match next.context("no start from the control box in time")? {
            Some(Ok(Message::Text(t))) => t,
            Some(Ok(Message::Close(_)) | Err(_)) | None => bail!("the link closed before start"),
            Some(Ok(_)) => continue,
        };
        match serde_json::from_str(&text)? {
            ToWorker::Refused { why } => bail!("the control box refused the ticket: {why}"),
            start @ ToWorker::Start { .. } => return Ok(start),
            _ => continue,
        }
    }
}

/// A link that started: tried again on a failed connection (not on a refusal) until
/// `patience.give_up`.
async fn started(url: &str, ticket: &str, patience: Patience) -> Result<(Ws, ToWorker)> {
    let until = Instant::now() + patience.give_up;
    loop {
        match connect(url, ticket, 0).await {
            Ok(mut ws) => {
                let start = start_of(&mut ws).await?;
                return Ok((ws, start));
            }
            Err(e) if Instant::now() + patience.retry < until => {
                eprintln!("worker: could not reach {url} ({e:#}); trying again");
            }
            Err(e) => return Err(e).with_context(|| format!("could not reach {url}")),
        }
        tokio::time::sleep(patience.retry).await;
    }
}

/// A worker's open link: what `Start` said, the branch's records, the control box's pause,
/// resume and stop, and the link itself.
pub struct Opened {
    pub plan: BranchPlan,
    /// The worker token: what a reconnect says hello with, and what the gateway forwards with.
    pub token: String,
    /// The control box's model gateway.
    pub gateway: String,
    pub records: Arc<LinkRecords>,
    /// Pause, resume and stop from the control box, in order.
    pub controls: UnboundedReceiver<ToWorker>,
    /// Ends Ok once the branch's end is recorded; an error when the link gave up or was refused.
    pub link: JoinHandle<Result<()>>,
}

/// Opens the link at `url` with `ticket`.
pub async fn open(url: &str, ticket: &str, patience: Patience) -> Result<Opened> {
    let (ws, start) = started(url, ticket, patience).await?;
    let ToWorker::Start {
        plan,
        token,
        gateway,
    } = start
    else {
        bail!("the first answer was not start");
    };
    let (records, requests) = LinkRecords::new(plan.branch_id.clone());
    let (controls_to, controls) = mpsc::unbounded_channel();
    let link = Link {
        url: url.to_string(),
        token: token.clone(),
        patience,
        requests: Some(requests),
        controls: controls_to,
        outbox: Outbox::default(),
        down_since: None,
    };
    Ok(Opened {
        plan: *plan,
        token,
        gateway,
        records: Arc::new(records),
        controls,
        link: tokio::spawn(link.serve(ws)),
    })
}

/// The worker's end of the link, on its own task.
struct Link {
    url: String,
    token: String,
    patience: Patience,
    /// None once every records handle is gone.
    requests: Option<UnboundedReceiver<Request>>,
    controls: UnboundedSender<ToWorker>,
    outbox: Outbox,
    /// When the link went down, until the control box shows it works again.
    down_since: Option<Instant>,
}

/// True when the control box refused a rejoin because the branch's end is recorded: the worker
/// token died with the branch, or the branch is no longer running.
fn end_recorded(why: &str) -> bool {
    why.contains(REFUSED_TOKEN) || why.starts_with(REFUSED_ENDED)
}

/// How a connection ended.
enum Pumped {
    Lost,
    Refused(String),
}

/// What a connection waits on next.
enum Got {
    Request(Option<Request>),
    Heard(Option<Result<Message, tungstenite::Error>>),
}

/// The next request, None once there are no more (then it waits for ever).
async fn next_request(requests: &mut Option<UnboundedReceiver<Request>>) -> Option<Request> {
    let Some(rx) = requests else {
        return std::future::pending().await;
    };
    let got = rx.recv().await;
    if got.is_none() {
        *requests = None;
    }
    got
}

impl Link {
    /// Keeps the link up until the control box has the branch's end: a lost connection is
    /// joined again; a refusal once the end was sent means the end was recorded (the token died
    /// with the branch).
    async fn serve(mut self, mut ws: Ws) -> Result<()> {
        loop {
            match self.pump(&mut ws).await {
                Pumped::Refused(why) if self.outbox.end_sent() && end_recorded(&why) => {
                    self.outbox.finish();
                    return Ok(());
                }
                Pumped::Refused(why) => bail!("the control box refused the link: {why}"),
                Pumped::Lost => {}
            }
            // The first try after the link worked goes at once; every later one waits.
            let at_once = self.down_since.is_none();
            let since = *self.down_since.get_or_insert_with(Instant::now);
            ws = self.rejoin(since, at_once).await?;
            self.outbox.reconnected();
        }
    }

    /// A connection again, with the worker token and the last `n` acknowledged: every `retry`
    /// (the first `at_once`), until the link has been down `give_up` since `since`.
    async fn rejoin(&self, since: Instant, at_once: bool) -> Result<Ws> {
        let mut wait = !at_once;
        let mut last = None;
        loop {
            if wait {
                tokio::time::sleep(self.patience.retry).await;
            }
            wait = true;
            if since.elapsed() >= self.patience.give_up {
                let (after, why) = (self.patience.give_up, last.unwrap_or_default());
                bail!("gave up on the control box after {after:?} down ({why})");
            }
            match connect(&self.url, &self.token, self.outbox.acked()).await {
                Ok(ws) => return Ok(ws),
                Err(e) => {
                    eprintln!("worker: the link is down ({e:#}); trying again");
                    last = Some(format!("{e:#}"));
                }
            }
        }
    }

    /// Sends what may go and reads what comes until the connection ends. Reading also answers
    /// the control box's pings.
    async fn pump(&mut self, ws: &mut Ws) -> Pumped {
        loop {
            while let Some(text) = self.outbox.next_frame() {
                if ws.send(Message::text(text)).await.is_err() {
                    return Pumped::Lost;
                }
            }
            let got = tokio::select! {
                r = next_request(&mut self.requests) => Got::Request(r),
                m = ws.next() => Got::Heard(m),
            };
            let ended = match got {
                Got::Request(r) => {
                    r.into_iter().for_each(|r| self.outbox.take(r));
                    None
                }
                Got::Heard(m) => self.heard(m),
            };
            if let Some(ended) = ended {
                return ended;
            }
        }
    }

    /// Takes what the control box sent: Some when the connection ended with it.
    fn heard(&mut self, m: Option<Result<Message, tungstenite::Error>>) -> Option<Pumped> {
        let text = match m {
            Some(Ok(Message::Text(t))) => t,
            Some(Ok(Message::Close(_)) | Err(_)) | None => return Some(Pumped::Lost),
            // A ping proves the link works only when nothing waits on the control box: one that
            // pings but cannot append would otherwise reset the clock on every rejoin.
            Some(Ok(Message::Ping(_))) => {
                if !self.outbox.owes() {
                    self.down_since = None;
                }
                return None;
            }
            Some(Ok(_)) => return None,
        };
        match serde_json::from_str(&text) {
            Ok(ToWorker::Ack { n, seq }) => {
                self.down_since = None;
                self.outbox.ack(n, seq);
            }
            Ok(ToWorker::Refused { why }) => return Some(Pumped::Refused(why)),
            Ok(ToWorker::Start { .. }) => {}
            Ok(control) => {
                self.down_since = None;
                let _ = self.controls.send(control);
            }
            Err(e) => eprintln!("worker: a frame it cannot read: {e}"),
        }
        None
    }
}

/// The worker's own shaked on `hub`, reached at `base`: no database (its records go over the
/// link), [`WorkerLayer`] (no vault, no keys from the environment), its model calls sent on
/// `to` the control box.
fn worker_app(hub: parts::Hub, base: String, to: ForwardTo) -> Result<Arc<AppState>> {
    let store = Arc::new(Store::detached(Arc::new(WorkerLayer))?);
    let mut config = Config::with_providers(ProviderConfig::without_environment());
    config.self_base = base;
    config.forward_to = Some(to);
    let state = Arc::new(AppState::new(store, Arc::new(hub), config));
    crate::app::install_default_drivers(&state);
    Ok(state)
}

/// The worker's shaked ([`worker_app`]) on the hub from the environment, serving its agents' MCP
/// endpoint, gateway (forwarding `to` the control box) and proxy on 127.0.0.1.
async fn worker_state(to: ForwardTo) -> Result<Arc<AppState>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let hub = crate::serve::worker_hub().await?;
    let state = worker_app(hub, format!("http://{}", listener.local_addr()?), to)?;
    let agents = crate::mcp::router().merge(crate::gateway::worker_router());
    let agents = agents.with_state(state.clone());
    tokio::spawn(async move { axum::serve(listener, agents).await });
    crate::serve::start_proxy(&state).await?;
    Ok(state)
}

/// Pauses, resumes and stops the branch as the control box says; over the run's limit by the
/// control box's estimate, the branch stops and ends `budget_exceeded`.
async fn obey(
    state: Arc<AppState>,
    ctx: Arc<BranchCtx>,
    mut controls: UnboundedReceiver<ToWorker>,
) {
    while let Some(c) = controls.recv().await {
        let done = match &c {
            ToWorker::Pause => ctx.pause().await.map(|_| ()),
            ToWorker::Resume => ctx.resume().await.map(|_| ()),
            ToWorker::Stop => {
                runner::stop(&state, &ctx).await;
                Ok(())
            }
            ToWorker::OverBudget { why } => {
                ctx.budget.force_over(why.clone());
                runner::stop(&state, &ctx).await;
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = done {
            eprintln!("worker: {c:?} failed: {e:#}");
        }
    }
}

/// Runs the stood-up branch to its end. The branch goes first when both are ready; a link that
/// ended Ok recorded the branch's end, so the branch is let finish. A link lost for good stops the
/// flow and the world and fails.
async fn until_end(
    state: &Arc<AppState>,
    ctx: &Arc<BranchCtx>,
    mut link: JoinHandle<Result<()>>,
) -> Result<()> {
    let branch = runner::run_branch(state.clone(), ctx.clone(), StartPoint::default());
    tokio::pin!(branch);
    tokio::select! {
        biased;
        () = &mut branch => link.await?.context(
            "the branch finished, but its end may not be recorded on the control box",
        ),
        lost = &mut link => match lost? {
            Ok(()) => {
                branch.await;
                Ok(())
            }
            Err(e) => {
                ctx.flow.stop();
                ctx.world.stop().await;
                Err(e)
            }
        },
    }
}

/// Stands the branch up and drives it to its end ([`until_end`]).
async fn drive(o: Opened) -> Result<()> {
    let Opened {
        plan,
        token,
        gateway,
        records,
        controls,
        link,
    } = o;
    let token = Secret::new(token);
    let state = worker_state(ForwardTo { gateway, token }).await?;
    let ctx = match runner::stand_up(&state, &plan, records).await {
        Ok(ctx) => Arc::new(ctx),
        // Its end (`infra_error`) went over the link.
        Err(e) => bail!("the branch could not start: {e}"),
    };
    state.live.insert(ctx.clone());
    tokio::spawn(obey(state.clone(), ctx.clone(), controls));
    until_end(&state, &ctx, link).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn only_a_refusal_that_says_the_branch_ended_confirms_its_end() {
        assert!(end_recorded(REFUSED_TOKEN));
        assert!(end_recorded(&format!("{REFUSED_ENDED} passed")));
        assert!(!end_recorded("event 9 came after 3"));
        assert!(!end_recorded(
            "resume_from 9 is past the last event appended (3)"
        ));
    }

    #[test]
    fn the_workers_shaked_has_no_vault_and_no_environment_keys() {
        let dir = tempfile::tempdir().unwrap();
        let registry = parts::Registry::scan(&[]).unwrap();
        let hub = parts::Hub::new(registry, dir.path().into(), dir.path().into());
        let to = ForwardTo {
            gateway: "http://127.0.0.1:9/gateway".into(),
            token: Secret::new("sdw_t"),
        };
        let state = worker_app(hub, "http://127.0.0.1:9".into(), to).unwrap();
        let Err(refused) = state.vault() else {
            panic!("no vault in a worker");
        };
        assert!(refused.contains("worker"), "{refused}");
        assert!(!state.store.layer().policy().environment_keys);
        assert!(!state.config.providers.machine_credentials);
        let to = state
            .config
            .forward_to
            .as_ref()
            .expect("model calls go to the control box");
        assert_eq!(to.gateway, "http://127.0.0.1:9/gateway");
    }

    #[test]
    fn worker_reads_its_flags() {
        let a = WorkerArgs::parse(&args(&["--link", "wss://x/worker", "--ticket-file", "/t"]));
        let want = WorkerArgs {
            link: "wss://x/worker".into(),
            ticket_file: "/t".into(),
        };
        assert_eq!(a.unwrap(), want);
        assert!(WorkerArgs::parse(&args(&["--link", "wss://x/worker"])).is_err());
        assert!(WorkerArgs::parse(&args(&["--link"])).is_err());
        assert!(WorkerArgs::parse(&args(&["--nope", "1"])).is_err());
    }

    #[tokio::test]
    async fn the_ticket_is_waited_for_read_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ticket");
        let (p, write) = (path.clone(), async {
            tokio::time::sleep(TICKET_POLL * 2).await;
            std::fs::write(&path, "sdt_ab").unwrap();
            tokio::time::sleep(TICKET_POLL * 3).await;
            assert!(
                path.exists(),
                "a ticket without its newline is still being written"
            );
            std::fs::write(&path, "sdt_abc\n").unwrap();
        });
        let (got, ()) = tokio::join!(take_ticket(&p), write);
        assert_eq!(got.unwrap(), "sdt_abc");
        assert!(!p.exists());
    }
}
