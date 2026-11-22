//! A worker's branch records: each event goes to the control box over the link and is recorded
//! once the control box acknowledges it, with the seq it assigned. The link keeps at most
//! [`WINDOW`] events unacknowledged on the wire; the rest wait their turn, and a reconnect resends
//! every one not acknowledged, in `n` order.
//!
//! **Events too large for one frame.** The control box closes a link that sends a frame over
//! [`MAX_LINK_FRAME`] without saying why, so the same event resent would close it again and
//! again. An event whose frame would pass [`MAX_EVENT_FRAME`] is cut down first (see
//! [`event_frame`]); what the worker keeps of it is what the control box records.

use std::{collections::VecDeque, sync::Mutex};

use anyhow::{anyhow, Result};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use super::{
    frames::{ToControl, WINDOW},
    link::MAX_LINK_FRAME,
};
use crate::{
    exec::BranchRecords,
    model::*,
    overhead::Overhead,
    store::{with_overhead, BranchId},
};

/// The largest `Event` frame a worker sends: the link's limit, less room to spare.
pub const MAX_EVENT_FRAME: usize = MAX_LINK_FRAME - 64 * 1024;
/// How many bytes of each long string a cut-down payload keeps.
pub const KEPT_STRING: usize = 16 * 1024;
/// How many bytes of its JSON text a payload replaced by its summary keeps.
pub const KEPT_TEXT: usize = 64 * 1024;

const LINK_GONE: &str = "the link to the control box is gone";

/// What the records ask of the link.
pub(super) enum Request {
    /// Send the `Event` frame `text` of event `n`; `acked` gets its seq.
    Event {
        n: u64,
        text: String,
        acked: oneshot::Sender<i64>,
    },
    /// Send the `Ended` frame `text` once every event is acknowledged; `done` fires once the
    /// control box has it.
    Ended {
        text: String,
        done: oneshot::Sender<()>,
    },
}

/// Who an event is from and what it is: the frame's fields besides `n` and the payload.
struct Head<'a> {
    agent: Option<&'a str>,
    kind: EventKind,
    overhead: Option<Overhead>,
}

/// An `Event` frame's text, and the payload it carries.
pub struct Framed {
    pub text: String,
    pub payload: Value,
}

fn framed(n: u64, head: &Head, payload: Value) -> Result<Framed> {
    let f = ToControl::Event {
        n,
        agent: head.agent.map(str::to_string),
        kind: head.kind,
        payload,
        overhead: head.overhead,
    };
    let text = serde_json::to_string(&f)?;
    let ToControl::Event { payload, .. } = f else {
        unreachable!("an event frame");
    };
    Ok(Framed { text, payload })
}

/// The `Event` frame of event `n`. The payload goes as it is when the frame fits in
/// [`MAX_EVENT_FRAME`] bytes. Else every string in it longer than [`KEPT_STRING`] bytes is cut
/// to that (at a character boundary) and the payload marked `truncated: true` (an object gets
/// the key; anything else becomes `{truncated: true, value}`). When even that does not fit, the
/// payload is replaced by `{truncated: true, bytes, head}`: its size as JSON and the first
/// [`KEPT_TEXT`] bytes of that JSON.
pub fn event_frame(
    n: u64,
    agent: Option<&str>,
    kind: EventKind,
    payload: Value,
    overhead: Option<Overhead>,
) -> Result<Framed> {
    let head = Head {
        agent,
        kind,
        overhead,
    };
    let whole = framed(n, &head, payload)?;
    if whole.text.len() <= MAX_EVENT_FRAME {
        return Ok(whole);
    }
    let cut = framed(n, &head, marked(cut_strings(whole.payload.clone())))?;
    if cut.text.len() <= MAX_EVENT_FRAME {
        return Ok(cut);
    }
    framed(n, &head, summary(&whole.payload)?)
}

/// `s` cut to at most `max` bytes, at a character boundary.
fn prefix(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// `v` with every string longer than [`KEPT_STRING`] bytes cut to that.
fn cut_strings(v: Value) -> Value {
    match v {
        Value::String(s) if s.len() > KEPT_STRING => Value::String(prefix(&s, KEPT_STRING).into()),
        Value::Array(a) => Value::Array(a.into_iter().map(cut_strings).collect()),
        Value::Object(o) => {
            Value::Object(o.into_iter().map(|(k, v)| (k, cut_strings(v))).collect())
        }
        other => other,
    }
}

fn marked(v: Value) -> Value {
    match v {
        Value::Object(mut o) => {
            o.insert("truncated".into(), Value::Bool(true));
            Value::Object(o)
        }
        other => json!({"truncated": true, "value": other}),
    }
}

fn summary(payload: &Value) -> Result<Value> {
    let text = serde_json::to_string(payload)?;
    let head = prefix(&text, KEPT_TEXT);
    Ok(json!({"truncated": true, "bytes": text.len(), "head": head}))
}

/// The link's side of the records: the events not yet acknowledged in `n` order, the first
/// `on_wire` of them sent on this connection, the last `n` acknowledged, and the end once the
/// branch has ended.
#[derive(Default)]
pub(super) struct Outbox {
    waiting: VecDeque<Waiting>,
    on_wire: usize,
    acked: u64,
    end: Option<End>,
}

struct Waiting {
    n: u64,
    text: String,
    acked: oneshot::Sender<i64>,
}

struct End {
    text: String,
    /// Sent on this connection.
    sent: bool,
    /// Sent on some connection.
    ever_sent: bool,
    done: Option<oneshot::Sender<()>>,
}

impl Outbox {
    pub(super) fn take(&mut self, r: Request) {
        match r {
            Request::Event { n, text, acked } => self.waiting.push_back(Waiting { n, text, acked }),
            Request::Ended { text, done } => {
                let (sent, ever_sent, done) = (false, false, Some(done));
                self.end = Some(End {
                    text,
                    sent,
                    ever_sent,
                    done,
                });
            }
        }
    }

    /// The next frame to send on this connection: an event while fewer than [`WINDOW`] are on
    /// the wire, then the end once every event is acknowledged.
    pub(super) fn next_frame(&mut self) -> Option<String> {
        if self.on_wire < self.waiting.len() && (self.on_wire as u64) < WINDOW {
            self.on_wire += 1;
            return Some(self.waiting[self.on_wire - 1].text.clone());
        }
        let end = self.end.as_mut().filter(|e| !e.sent)?;
        if !self.waiting.is_empty() {
            return None;
        }
        (end.sent, end.ever_sent) = (true, true);
        Some(end.text.clone())
    }

    /// Event `n` was acknowledged with `seq` (an acknowledgement of anything but the oldest
    /// event waiting is one already taken, and ignored).
    pub(super) fn ack(&mut self, n: u64, seq: i64) {
        if self.waiting.front().is_none_or(|w| w.n != n) || self.on_wire == 0 {
            return;
        }
        let w = self.waiting.pop_front().expect("the front was there");
        self.on_wire -= 1;
        self.acked = n;
        let _ = w.acked.send(seq);
    }

    /// A new connection: nothing is on its wire yet.
    pub(super) fn reconnected(&mut self) {
        self.on_wire = 0;
        if let Some(e) = &mut self.end {
            e.sent = false;
        }
    }

    /// The last `n` the control box acknowledged.
    pub(super) fn acked(&self) -> u64 {
        self.acked
    }

    /// True while something waits on the control box: an event not acknowledged, or the end
    /// not yet confirmed.
    pub(super) fn owes(&self) -> bool {
        !self.waiting.is_empty() || self.end.is_some()
    }

    /// True once the end was sent on some connection.
    pub(super) fn end_sent(&self) -> bool {
        self.end.as_ref().is_some_and(|e| e.ever_sent)
    }

    /// The control box has the end.
    pub(super) fn finish(&mut self) {
        if let Some(done) = self.end.as_mut().and_then(|e| e.done.take()) {
            let _ = done.send(());
        }
    }
}

/// The next `n`, and where requests go, under one lock: requests reach the link in `n` order.
struct Sending {
    last_n: u64,
    to: mpsc::UnboundedSender<Request>,
}

/// A branch's records over a worker's link (see the module).
pub struct LinkRecords {
    branch_id: BranchId,
    sending: Mutex<Sending>,
    /// What this worker recorded, in seq order.
    recorded: Mutex<Vec<Event>>,
}

impl LinkRecords {
    /// The records of `branch_id`, and the requests they make of the link.
    pub(super) fn new(branch_id: BranchId) -> (LinkRecords, mpsc::UnboundedReceiver<Request>) {
        let (to, requests) = mpsc::unbounded_channel();
        let records = LinkRecords {
            branch_id,
            sending: Mutex::new(Sending { last_n: 0, to }),
            recorded: Mutex::default(),
        };
        (records, requests)
    }

    /// Sends the event to the link: the payload sent, and where its seq will come.
    fn send(&self, head: &Head, payload: Value) -> Result<(Value, oneshot::Receiver<i64>)> {
        let mut s = self.sending.lock().unwrap();
        let n = s.last_n + 1;
        let f = event_frame(n, head.agent, head.kind, payload, head.overhead)?;
        let (acked, seq) = oneshot::channel();
        let text = f.text;
        let request = Request::Event { n, text, acked };
        s.to.send(request).map_err(|_| anyhow!(LINK_GONE))?;
        s.last_n = n;
        Ok((f.payload, seq))
    }

    fn keep(&self, ev: Event) {
        let mut recorded = self.recorded.lock().unwrap();
        let at = recorded.partition_point(|e| e.seq < ev.seq);
        recorded.insert(at, ev);
    }

    async fn append_now(&self, head: Head<'_>, payload: Value) -> Result<Event> {
        let (payload, seq) = self.send(&head, payload)?;
        let seq = seq.await.map_err(|_| anyhow!(LINK_GONE))?;
        let ev = Event {
            branch_id: self.branch_id.clone(),
            seq,
            at: chrono::Utc::now(),
            agent: head.agent.map(str::to_string),
            kind: head.kind,
            payload: with_overhead(payload, head.overhead)?,
        };
        self.keep(ev.clone());
        Ok(ev)
    }

    async fn end(&self, status: BranchStatus, error: Option<String>) -> Result<()> {
        if status == BranchStatus::Running {
            return Err(anyhow!("a worker's branch is never set running"));
        }
        let text = serde_json::to_string(&ToControl::Ended { status, error })?;
        let (done, confirmed) = oneshot::channel();
        let sent = {
            let sending = self.sending.lock().unwrap();
            sending.to.send(Request::Ended { text, done })
        };
        sent.map_err(|_| anyhow!(LINK_GONE))?;
        confirmed.await.map_err(|_| anyhow!(LINK_GONE))
    }
}

impl BranchRecords for LinkRecords {
    fn append<'a>(
        &'a self,
        agent: Option<&'a str>,
        kind: EventKind,
        payload: Value,
        overhead: Option<Overhead>,
    ) -> BoxFuture<'a, Result<Event>> {
        let head = Head {
            agent,
            kind,
            overhead,
        };
        Box::pin(self.append_now(head, payload))
    }

    /// What this worker recorded. A run's first branch has no history before it; a fork's
    /// (M2b) comes with its plan.
    fn events(&self) -> BoxFuture<'_, Result<Vec<Event>>> {
        let recorded = self.recorded.lock().unwrap().clone();
        Box::pin(async move { Ok(recorded) })
    }

    /// Sends `Ended`; returns once the control box has it.
    fn set_status(&self, status: BranchStatus, error: Option<String>) -> BoxFuture<'_, Result<()>> {
        Box::pin(self.end(status, error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(n: u64) -> (Request, oneshot::Receiver<i64>) {
        let (acked, seq) = oneshot::channel();
        let text = format!("event {n}");
        (Request::Event { n, text, acked }, seq)
    }

    fn drained(o: &mut Outbox) -> Vec<String> {
        std::iter::from_fn(|| o.next_frame()).collect()
    }

    #[test]
    fn at_most_a_window_of_events_is_on_the_wire() {
        let mut o = Outbox::default();
        let seqs: Vec<_> = (1..=WINDOW + 2)
            .map(|n| {
                let (r, seq) = event(n);
                o.take(r);
                seq
            })
            .collect();
        assert_eq!(drained(&mut o).len() as u64, WINDOW);
        o.ack(1, 10);
        assert_eq!(drained(&mut o), [format!("event {}", WINDOW + 1)]);
        assert_eq!(o.acked(), 1);
        let mut seqs = seqs.into_iter();
        assert_eq!(seqs.next().unwrap().try_recv(), Ok(10));
        assert!(
            seqs.next().unwrap().try_recv().is_err(),
            "2 is not acked yet"
        );
    }

    #[test]
    fn an_empty_outbox_owes_nothing() {
        let mut o = Outbox::default();
        assert!(!o.owes());
        o.take(event(1).0);
        assert!(o.owes());
        drained(&mut o);
        o.ack(1, 1);
        assert!(!o.owes());
    }

    #[test]
    fn a_reconnect_resends_what_was_not_acknowledged_in_order() {
        let mut o = Outbox::default();
        for n in 1..=3 {
            o.take(event(n).0);
        }
        assert_eq!(drained(&mut o).len(), 3);
        o.ack(1, 1);
        o.ack(1, 1); // again: ignored
        o.ack(3, 3); // out of order: ignored
        o.reconnected();
        assert_eq!(drained(&mut o), ["event 2", "event 3"]);
        assert_eq!(o.acked(), 1);
    }

    #[test]
    fn the_end_goes_once_every_event_is_acknowledged() {
        let mut o = Outbox::default();
        o.take(event(1).0);
        let (done, mut confirmed) = oneshot::channel();
        o.take(Request::Ended {
            text: "ended".into(),
            done,
        });
        assert_eq!(drained(&mut o), ["event 1"]);
        assert!(!o.end_sent() && o.owes());
        o.ack(1, 1);
        assert_eq!(drained(&mut o), ["ended"]);
        assert!(o.end_sent());
        assert!(o.owes(), "the end is not confirmed yet");
        o.reconnected();
        assert_eq!(drained(&mut o), ["ended"], "sent again on a new connection");
        assert!(confirmed.try_recv().is_err());
        o.finish();
        assert_eq!(confirmed.try_recv(), Ok(()));
    }

    fn sent(payload: Value) -> (Framed, Value) {
        let f = event_frame(7, Some("a"), EventKind::ToolCall, payload, None).unwrap();
        let v: Value = serde_json::from_str(&f.text).unwrap();
        assert!(f.text.len() <= MAX_EVENT_FRAME, "{}", f.text.len());
        assert_eq!(v["payload"], f.payload, "the frame carries what is kept");
        (f, v)
    }

    #[test]
    fn a_small_payload_goes_as_it_is() {
        let p = json!({"name": "poke", "args": {}});
        assert_eq!(sent(p.clone()).0.payload, p);
    }

    #[test]
    fn a_payload_too_large_for_a_frame_has_its_long_strings_cut() {
        let big = "é".repeat(MAX_LINK_FRAME); // two bytes each
        let p = json!({"name": "poke", "result": {"text": big}, "note": "kept"});
        let (f, _) = sent(p);
        assert_eq!(f.payload["truncated"], json!(true));
        assert_eq!(f.payload["note"], "kept");
        let text = f.payload["result"]["text"].as_str().unwrap();
        assert_eq!(text.len(), KEPT_STRING, "cut at a character boundary");
        let (f, _) = sent(json!("x".repeat(MAX_LINK_FRAME)));
        assert_eq!(f.payload["value"].as_str().unwrap().len(), KEPT_STRING);
    }

    #[test]
    fn a_payload_of_many_short_strings_is_summarised() {
        let many: Vec<String> = (0..1000)
            .map(|i| format!("{i}{}", "y".repeat(10_000)))
            .collect();
        let p = json!({"rows": many});
        let bytes = serde_json::to_string(&p).unwrap().len();
        let (f, _) = sent(p);
        assert_eq!(f.payload["truncated"], json!(true));
        assert_eq!(f.payload["bytes"], json!(bytes));
        assert_eq!(f.payload["head"].as_str().unwrap().len(), KEPT_TEXT);
    }
}
