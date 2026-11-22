//! The worker links this control box keeps, by branch id: the plan each branch's worker is
//! started from, who the branch records for, the connection open now (if one is) and the events
//! appended so far, so a resent event is acknowledged again instead of appended twice.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use tokio::sync::mpsc;

use super::{
    frames::{ToWorker, WINDOW},
    tickets::Tickets,
};
use crate::{auth::Caller, runner::BranchPlan};

/// Every branch a worker runs (or is about to) for this control box, and the tickets issued for
/// them.
#[derive(Default)]
pub struct Links {
    pub tickets: Tickets,
    branches: Mutex<HashMap<String, Arc<Link>>>,
}

/// One branch's link.
pub struct Link {
    pub plan: BranchPlan,
    wire: Mutex<Wire>,
    pub(super) appended: tokio::sync::Mutex<Appended>,
}

/// Who the branch records for (set when its ticket is redeemed) and the open connection's
/// generation and relay.
#[derive(Default)]
struct Wire {
    caller: Option<Arc<Caller>>,
    generation: u64,
    out: Option<mpsc::UnboundedSender<ToWorker>>,
}

/// The last `n` appended and the seqs of the last [`WINDOW`] appended events: a worker never
/// resends an older one.
#[derive(Default)]
pub(super) struct Appended {
    pub(super) last_n: u64,
    recent: VecDeque<(u64, i64)>,
}

impl Appended {
    /// The seq event `n` got, while it is one of the last [`WINDOW`].
    pub(super) fn seq_of(&self, n: u64) -> Option<i64> {
        self.recent.iter().find(|(m, _)| *m == n).map(|(_, s)| *s)
    }

    /// The seq of event `n` when it was appended already, None when it is the next one, and why
    /// not when it is neither.
    pub(super) fn resent(&self, n: u64) -> Result<Option<i64>, String> {
        match n <= self.last_n {
            true => (self.seq_of(n).map(Some)).ok_or(format!("event {n} is older than the window")),
            false if n == self.last_n + 1 => Ok(None),
            false => Err(format!("event {n} came after {}", self.last_n)),
        }
    }

    pub(super) fn push(&mut self, n: u64, seq: i64) {
        self.last_n = n;
        self.recent.push_back((n, seq));
        if self.recent.len() as u64 > WINDOW {
            self.recent.pop_front();
        }
    }
}

impl Links {
    /// A worker for `plan`'s branch will say hello: its ticket's `Start` carries `plan`.
    pub fn expect(&self, plan: BranchPlan) {
        let link = Arc::new(Link {
            plan,
            wire: Mutex::default(),
            appended: tokio::sync::Mutex::default(),
        });
        let id = link.plan.branch_id.clone();
        self.branches.lock().unwrap().insert(id, link);
    }

    /// Sends `f` to `branch`'s worker: false when no worker is connected.
    pub fn send(&self, branch: &str, f: ToWorker) -> bool {
        let Some(link) = self.get(branch) else {
            return false;
        };
        let wire = link.wire.lock().unwrap();
        wire.out.as_ref().is_some_and(|out| out.send(f).is_ok())
    }

    /// True when `branch`'s worker is connected now.
    pub fn connected(&self, branch: &str) -> bool {
        self.get(branch)
            .is_some_and(|l| l.wire.lock().unwrap().out.is_some())
    }

    pub(super) fn get(&self, branch: &str) -> Option<Arc<Link>> {
        self.branches.lock().unwrap().get(branch).cloned()
    }

    /// The branch ended: its link is no longer kept.
    pub(super) fn forget(&self, branch: &str) {
        self.branches.lock().unwrap().remove(branch);
    }
}

impl Link {
    /// Who the branch records for, once its worker has started.
    pub(super) fn caller(&self) -> Option<Arc<Caller>> {
        self.wire.lock().unwrap().caller.clone()
    }

    /// Marks the branch started for `caller`: false when it had started already.
    pub(super) fn start_for(&self, caller: Arc<Caller>) -> bool {
        let mut wire = self.wire.lock().unwrap();
        if wire.caller.is_some() {
            return false;
        }
        wire.caller = Some(caller);
        true
    }

    /// Makes a new connection the open one (a connection still open is dropped from the relay):
    /// its generation and the frames relayed to it.
    pub(super) fn attach(&self) -> (u64, mpsc::UnboundedReceiver<ToWorker>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut wire = self.wire.lock().unwrap();
        wire.generation += 1;
        wire.out = Some(tx);
        (wire.generation, rx)
    }

    /// True when the connection of `generation` is still the open one.
    pub(super) fn is_current(&self, generation: u64) -> bool {
        self.wire.lock().unwrap().generation == generation
    }

    /// The connection of `generation` closed: nothing is relayed until the worker reconnects.
    pub(super) fn detach(&self, generation: u64) {
        let mut wire = self.wire.lock().unwrap();
        if wire.generation == generation {
            wire.out = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_keeps_the_last_window_of_seqs() {
        let mut a = Appended::default();
        for n in 1..=WINDOW + 2 {
            a.push(n, n as i64 + 10);
        }
        assert_eq!(a.last_n, WINDOW + 2);
        assert_eq!(a.seq_of(WINDOW + 2), Some(WINDOW as i64 + 12));
        assert_eq!(a.seq_of(3), Some(13));
        assert_eq!(a.seq_of(2), None);
        assert_eq!(a.resent(3), Ok(Some(13)));
        assert_eq!(a.resent(WINDOW + 3), Ok(None));
        assert!(a.resent(2).is_err() && a.resent(WINDOW + 4).is_err());
    }
}
