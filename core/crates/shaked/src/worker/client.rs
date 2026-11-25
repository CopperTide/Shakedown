//! The worker's end of its link to the control box: it says hello with the ticket, waits for
//! `Start`, then keeps the link up on its own task ([`open`]).
//!
//! A dropped link is tried again at once, then every [`Patience::retry`], with the worker token
//! and the last `n` acknowledged; every event not acknowledged is sent again, in order. The link
//! counts as down from the drop until the control box shows it works again on the new connection:
//! an ack, a pause, resume or stop, or a ping while nothing waits on it (no event unacknowledged,
//! no end unconfirmed). A connection that is accepted and then closed, or that pings but never
//! acknowledges, does not reset it. After [`Patience::give_up`] down the worker stops the world
//! and exits 1 (the control box ends the branch). `SHAKEDOWN_WORKER_RETRY_MS` and
//! `SHAKEDOWN_WORKER_GIVE_UP_MS` override both (tests).

use std::{sync::Arc, time::Duration};

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
    link_records::{LinkRecords, Outbox, Request},
};
use crate::{providers::NamedKey, runner::BranchPlan};

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// How long a connection attempt may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the control box may take to answer a ticket's hello with `Start`.
const START_TIMEOUT: Duration = Duration::from_secs(60);

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
    /// The keys the team's agents name, and the providers they key (no secrets).
    pub keys: Vec<NamedKey>,
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
        keys,
    } = start
    else {
        bail!("the first answer was not start");
    };
    let (records, requests) = LinkRecords::new(plan.branch_id.clone());
    let (controls_to, controls) = mpsc::unbounded_channel();
    let link = LinkClient::new(url, &token, patience, (requests, controls_to));
    Ok(Opened {
        plan: *plan,
        token,
        gateway,
        keys,
        records: Arc::new(records),
        controls,
        link: tokio::spawn(link.serve(ws)),
    })
}

/// The worker's end of the link, on its own task.
struct LinkClient {
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

impl LinkClient {
    /// The link to `url` with worker token `token`, nothing sent yet: the branch's records'
    /// `requests` go out, the control box's controls to `controls`.
    fn new(
        url: &str,
        token: &str,
        patience: Patience,
        (requests, controls): (UnboundedReceiver<Request>, UnboundedSender<ToWorker>),
    ) -> LinkClient {
        LinkClient {
            url: url.to_string(),
            token: token.to_string(),
            patience,
            requests: Some(requests),
            controls,
            outbox: Outbox::default(),
            down_since: None,
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_refusal_that_says_the_branch_ended_confirms_its_end() {
        assert!(end_recorded(REFUSED_TOKEN));
        assert!(end_recorded(&format!("{REFUSED_ENDED} passed")));
        assert!(!end_recorded("event 9 came after 3"));
        assert!(!end_recorded(
            "resume_from 9 is past the last event appended (3)"
        ));
    }
}
