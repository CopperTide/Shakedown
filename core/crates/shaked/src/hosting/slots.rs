//! How many sandboxes a scope (the layer's `object_prefix`) runs at a time: at most
//! [`PER_SCOPE`], counted on this control box. A run beyond that waits its turn, first come first
//! served per scope, up to [`QUEUE_PER_SCOPE`] waiting (beyond that it is refused before anything
//! is recorded). A slot is given back when its sandbox has been killed, and goes straight to the
//! scope's first waiter.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use axum::http::StatusCode;
use tokio::sync::oneshot;

use crate::layer::Refusal;

/// Sandboxes a scope may run at once.
pub const PER_SCOPE: usize = 1;
/// Runs a scope may have waiting for a slot.
pub const QUEUE_PER_SCOPE: usize = 50;

/// Why a run beyond the runs a scope may have waiting ([`QUEUE_PER_SCOPE`]) is refused.
pub const BUSY: &str =
    "too many cloud runs of yours are waiting already (one runs at a time): start this one when fewer wait";

/// One scope's sandboxes running and runs waiting, oldest first.
#[derive(Debug, Default)]
struct Scope {
    running: usize,
    waiting: VecDeque<Waiter>,
}

#[derive(Debug)]
struct Waiter {
    branch: String,
    turn: oneshot::Sender<Slot>,
}

type Shared = Arc<Mutex<HashMap<String, Scope>>>;

/// The sandboxes each scope runs and the runs it has waiting, by scope; at most `bound` waiting
/// in a scope.
#[derive(Debug)]
pub(super) struct Slots {
    shared: Shared,
    bound: usize,
}

impl Default for Slots {
    fn default() -> Slots {
        Slots::bounded(QUEUE_PER_SCOPE)
    }
}

/// One sandbox of `scope` counted, until dropped.
#[derive(Debug)]
pub struct Slot {
    taken: Shared,
    scope: String,
}

/// A run's turn at a slot: now, or once the runs before it are done ([`Slots::wait`]).
#[derive(Debug)]
pub enum Turn {
    Now(Slot),
    Later,
}

impl Slots {
    /// At most `bound` runs waiting in a scope.
    pub(super) fn bounded(bound: usize) -> Slots {
        Slots {
            shared: Shared::default(),
            bound,
        }
    }

    /// A slot for `scope` now, else [`Turn::Later`] (the run, once recorded, [`Slots::wait`]s);
    /// refused 429 ([`BUSY`]) when its bound ([`QUEUE_PER_SCOPE`]) of runs wait already.
    pub(super) fn take(&self, scope: &str) -> Result<Turn, Refusal> {
        let mut scopes = self.shared.lock().unwrap();
        let s = scopes.entry(scope.to_string()).or_default();
        if s.running < PER_SCOPE && s.waiting.is_empty() {
            s.running += 1;
            return Ok(Turn::Now(self.slot(scope)));
        }
        if s.waiting.len() >= self.bound {
            return Err(Refusal::new(StatusCode::TOO_MANY_REQUESTS, BUSY));
        }
        Ok(Turn::Later)
    }

    /// Queues `branch` last in `scope`: its slot comes once the runs before it are done (an
    /// error when it is [`cancel`](Self::cancel)led first). A slot freed meanwhile is its at once.
    pub(super) fn wait(&self, scope: &str, branch: &str) -> oneshot::Receiver<Slot> {
        let (turn, rx) = oneshot::channel();
        let mut scopes = self.shared.lock().unwrap();
        let s = scopes.entry(scope.to_string()).or_default();
        let waiter = Waiter {
            branch: branch.to_string(),
            turn,
        };
        match s.running < PER_SCOPE && s.waiting.is_empty() {
            true => {
                s.running += 1;
                let _ = waiter.turn.send(self.slot(scope));
            }
            false => s.waiting.push_back(waiter),
        }
        rx
    }

    /// Takes `branch` out of its queue: whether it was waiting.
    pub(super) fn cancel(&self, branch: &str) -> bool {
        let mut scopes = self.shared.lock().unwrap();
        let found = scopes.values_mut().find_map(|s| {
            let at = s.waiting.iter().position(|w| w.branch == branch)?;
            s.waiting.remove(at)
        });
        found.is_some()
    }

    /// `branch`'s place in its queue, from 1; None when it does not wait.
    pub fn position(&self, branch: &str) -> Option<usize> {
        let scopes = self.shared.lock().unwrap();
        let mut queues = scopes.values().map(|s| &s.waiting);
        queues.find_map(|q| q.iter().position(|w| w.branch == branch).map(|i| i + 1))
    }

    fn slot(&self, scope: &str) -> Slot {
        Slot {
            taken: self.shared.clone(),
            scope: scope.to_string(),
        }
    }
}

impl Drop for Slot {
    /// Hands the slot to the scope's first waiter still waiting, else gives it back.
    fn drop(&mut self) {
        let mut scopes = self.taken.lock().unwrap();
        let Some(s) = scopes.get_mut(&self.scope) else {
            return;
        };
        while let Some(w) = s.waiting.pop_front() {
            if w.turn.is_closed() {
                continue;
            }
            let next = Slot {
                taken: self.taken.clone(),
                scope: self.scope.clone(),
            };
            match w.turn.send(next) {
                Ok(()) => return,
                // Its waiter just went: this slot is the one being dropped, never counted twice.
                Err(unsent) => std::mem::forget(unsent),
            }
        }
        s.running -= 1;
        if s.running == 0 {
            scopes.remove(&self.scope);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now(t: Turn) -> Slot {
        match t {
            Turn::Now(s) => s,
            Turn::Later => panic!("a slot now"),
        }
    }

    #[test]
    fn a_scope_runs_one_and_the_rest_wait_their_turn_in_order() {
        let slots = Slots::default();
        let a = now(slots.take("a").unwrap());
        assert!(matches!(slots.take("a").unwrap(), Turn::Later));
        let mut second = slots.wait("a", "b2");
        let mut third = slots.wait("a", "b3");
        assert_eq!(
            (slots.position("b2"), slots.position("b3")),
            (Some(1), Some(2))
        );
        let other = now(slots.take("b").unwrap());
        drop(a);
        let second = second.try_recv().expect("the first waiter's turn");
        assert!(third.try_recv().is_err(), "still waiting");
        assert_eq!(slots.position("b3"), Some(1));
        drop(second);
        let third = third.try_recv().expect("the next waiter's turn");
        drop(third);
        assert!(matches!(slots.take("a").unwrap(), Turn::Now(_)));
        drop(other);
    }

    #[test]
    fn a_cancelled_or_gone_waiter_is_skipped() {
        let slots = Slots::default();
        let a = now(slots.take("a").unwrap());
        let _cancelled = slots.wait("a", "c");
        let gone = slots.wait("a", "g");
        let mut last = slots.wait("a", "l");
        assert!(slots.cancel("c"));
        assert!(!slots.cancel("c"));
        drop(gone);
        drop(a);
        drop(last.try_recv().expect("the waiter still waiting"));
        assert!(
            slots.shared.lock().unwrap().is_empty(),
            "every slot given back"
        );
    }

    #[test]
    fn a_full_queue_refuses() {
        let slots = Slots::bounded(3);
        let _a = now(slots.take("a").unwrap());
        let waiting: Vec<_> = (0..3).map(|i| slots.wait("a", &i.to_string())).collect();
        assert_eq!(slots.take("a").unwrap_err().message, BUSY);
        assert!(slots.take("b").is_ok(), "another scope has its own");
        drop(waiting);
    }
}
