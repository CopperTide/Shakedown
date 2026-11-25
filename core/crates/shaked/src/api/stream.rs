//! A branch's events and final status as a server-sent event stream
//! (`GET /api/branches/{id}/stream`).

use crate::auth::{Caller, Who};
use std::{sync::Arc, time::Duration};

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{
        sse::{Event as SseEvent, KeepAlive, Sse},
        IntoResponse, Response,
    },
};
use serde_json::json;
use tokio::sync::{
    broadcast::{self, error::RecvError},
    mpsc,
};

use super::{branch_or_404, ApiError};
use crate::{app::AppState, model::*};

fn sse_event(name: &str, data: &impl serde::Serialize) -> SseEvent {
    SseEvent::default()
        .event(name)
        .json_data(data)
        .unwrap_or_else(|_| SseEvent::default().event(name).data("null"))
}

/// Frame ids let EventSource resume without replaying history after a disconnect.
fn event_frame(event: &Event, lean: bool) -> SseEvent {
    let mut event = event.clone();
    if lean && event.kind == EventKind::ModelCall {
        if let Some(payload) = event.payload.as_object_mut() {
            let model = payload.get("request").and_then(|r| r.get("model")).cloned();
            payload.insert(
                "request".into(),
                model.map_or(serde_json::Value::Null, |m| json!({"model": m})),
            );
            payload.insert("bodies_omitted".into(), json!(true));
            payload.insert("response".into(), serde_json::Value::Null);
            payload.remove("provider_response");
        }
    }
    sse_event("event", &event).id(event.seq.to_string())
}

#[derive(Default, serde::Deserialize)]
pub(super) struct StreamQuery {
    #[serde(default)]
    after: i64,
    lean: Option<String>,
}

type SseTx = mpsc::Sender<Result<SseEvent, std::convert::Infallible>>;

/// Feeds one branch's events and final status to an SSE client.
struct Pump {
    state: Arc<AppState>,
    caller: Arc<Caller>,
    id: String,
    /// The last event seq sent.
    last: i64,
    lean: bool,
    tx: SseTx,
}

/// What woke the pump.
enum Wake {
    Stop,
    Tick,
    Ev(Result<Event, RecvError>),
}

/// What the event pump does after a wake-up.
enum Next {
    /// Stop: shaked shuts down, or the client or the store is gone.
    Stop,
    /// See whether the branch has ended, then wait.
    CheckStatus,
    /// Wait for the next wake-up.
    Wait,
}

/// `then` when `ok`, else `Next::Stop`.
fn unless_gone(ok: bool, then: Next) -> Next {
    if ok {
        then
    } else {
        Next::Stop
    }
}

impl Pump {
    /// Sends the stored events after `last`; false when the client is gone or the store failed.
    async fn catch_up(&mut self) -> bool {
        let Ok(events) = self
            .state
            .store
            .at(&self.caller)
            .events_after(&self.id, self.last, self.lean)
            .await
        else {
            return false;
        };
        let from = self.last;
        for e in events.into_iter().filter(|e| e.seq > from) {
            self.last = e.seq;
            if self.tx.send(Ok(event_frame(&e, self.lean))).await.is_err() {
                return false;
            }
        }
        true
    }

    /// Handles one wake-up.
    async fn on_wake(&mut self, wake: Wake) -> Next {
        match wake {
            Wake::Stop => Next::Stop,
            Wake::Tick => Next::CheckStatus,
            Wake::Ev(Ok(e)) if e.branch_id == self.id => self.on_event(&e).await,
            Wake::Ev(Ok(_)) => Next::Wait,
            Wake::Ev(Err(RecvError::Lagged(_))) => {
                unless_gone(self.catch_up().await, Next::CheckStatus)
            }
            Wake::Ev(Err(RecvError::Closed)) => {
                tokio::time::sleep(Duration::from_millis(250)).await;
                Next::Wait
            }
        }
    }

    /// An event of this branch: sent when it is the next one; after a gap, the missed ones are
    /// read from the store.
    async fn on_event(&mut self, e: &Event) -> Next {
        let ok = if e.seq == self.last + 1 {
            self.last = e.seq;
            self.tx.send(Ok(event_frame(e, self.lean))).await.is_ok()
        } else {
            e.seq <= self.last || self.catch_up().await
        };
        unless_gone(ok, Next::Wait)
    }

    /// Sends the final `status` once the branch is no longer running; false to stop.
    async fn status_check(&mut self) -> bool {
        let Ok(Some(b)) = self
            .state
            .store
            .at(&self.caller)
            .find_branch(&self.id)
            .await
        else {
            return false;
        };
        if b.status.goes_on() {
            return true;
        }
        // Events written just before the status flipped may not have been seen yet.
        if self.catch_up().await {
            let status = json!({"status": b.status, "error": b.error});
            let _ = self.tx.send(Ok(sse_event("status", &status))).await;
        }
        false
    }

    async fn run(mut self, mut rx: broadcast::Receiver<Event>) {
        let mut shutdown = self.state.shutdown.subscribe();
        if *shutdown.borrow() || !self.catch_up().await {
            return;
        }
        // Status is checked on its own timer so other branches' events can't starve it,
        // and once right away so a finished branch gets its `status` immediately.
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            let wake = tokio::select! {
                _ = shutdown.changed() => Wake::Stop,
                _ = tick.tick() => Wake::Tick,
                got = rx.recv() => Wake::Ev(got),
            };
            match self.on_wake(wake).await {
                Next::Stop => return,
                Next::CheckStatus if !self.status_check().await => return,
                Next::CheckStatus | Next::Wait => {}
            }
        }
    }
}

pub(super) async fn stream(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
    Query(query): Query<StreamQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    branch_or_404(&state, &caller, &id).await?;
    // Subscribe before reading the stored events so nothing falls between the two.
    let rx = state.store.subscribe();
    let (tx, out) = mpsc::channel(64);
    let last = headers
        .get("last-event-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .max(query.after)
        .max(0);
    tokio::spawn(
        Pump {
            state,
            caller,
            id,
            last,
            lean: query
                .lean
                .as_deref()
                .is_some_and(|v| v == "1" || v == "true"),
            tx,
        }
        .run(rx),
    );
    let stream =
        futures_util::stream::unfold(out, |mut rx| async { rx.recv().await.map(|x| (x, rx)) });
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response())
}
