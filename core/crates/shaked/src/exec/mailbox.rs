//! What passes between a running branch's agents outside its events: each agent's latest
//! handoff note, and what agents say, passed on to a dispatch runner that listens.

use std::{collections::HashMap, sync::Mutex};

use super::Said;

/// A running branch's handoff notes and the tap on what its agents say.
#[derive(Default)]
pub struct Mailbox {
    /// agent id -> its latest handoff note.
    pub handoffs: Mutex<HashMap<String, String>>,
    /// Where what agents say is passed on, while a dispatch runner listens.
    tap: Mutex<Option<tokio::sync::mpsc::UnboundedSender<Said>>>,
}

impl Mailbox {
    /// Passes what agents say from now on to the returned receiver (replacing an earlier one).
    pub fn listen(&self) -> tokio::sync::mpsc::UnboundedReceiver<Said> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        *self.tap.lock().unwrap() = Some(tx);
        rx
    }

    /// Passes on what an agent said, when someone listens.
    pub fn tell(&self, said: Said) {
        if let Some(tx) = self.tap.lock().unwrap().as_ref() {
            let _ = tx.send(said);
        }
    }

    pub fn handoff_note(&self, agent: &str) -> Option<String> {
        self.handoffs.lock().unwrap().get(agent).cloned()
    }
}
