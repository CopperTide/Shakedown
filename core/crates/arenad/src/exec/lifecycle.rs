//! How far a running branch is through its life: its clock ticker, whether a world program
//! stopped, whether something began to end it, and whether its lead closed it.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

use crate::ticker::Ticker;

/// A running branch's ticker and end state.
#[derive(Default)]
pub struct Lifecycle {
    /// Sends `advance` to the world's clock parts while the branch runs.
    pub(super) ticker: tokio::sync::Mutex<Option<Ticker>>,
    /// Set when a program of the world has stopped: the branch ends `infra_error` with it.
    fatal: Mutex<Option<String>>,
    /// Set once something has begun to end the branch.
    ending: AtomicBool,
    /// Set once the lead of a dispatch run has finished it: every later call is refused.
    closed: AtomicBool,
}

impl Lifecycle {
    /// Refuses every later call: the run is over.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// True for the one caller that may end the branch: the first to ask.
    pub fn claim_end(&self) -> bool {
        !self.ending.swap(true, Ordering::SeqCst)
    }

    /// Why the branch can't go on (a world program stopped), once that has happened.
    pub fn fatal(&self) -> Option<String> {
        self.fatal.lock().unwrap().clone()
    }

    pub(crate) fn set_fatal(&self, msg: &str) {
        self.fatal
            .lock()
            .unwrap()
            .get_or_insert_with(|| msg.to_string());
    }
}
