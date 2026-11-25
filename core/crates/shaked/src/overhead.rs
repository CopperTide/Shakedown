//! Shakedown's own time, kept apart from the agents' and the world's so a run's numbers can estimate
//! production. Each piece of Shakedown's work adds its milliseconds to the branch's ledger (its
//! `BranchCtx::overhead`); the next event recorded for the branch carries what built up since the
//! one before, as `overhead_ms`, and the ledger starts again. Summing `overhead_ms` over a
//! branch's events gives the time Shakedown added to it.

use std::{sync::Mutex, time::Instant};

use serde::{Deserialize, Serialize};

/// The kinds of Shakedown work that are timed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat {
    /// Saving parts after a step (the world clock is paused meanwhile).
    Save,
    /// Starting a branch's world from a save, or by replay.
    Restore,
    /// Writing the record (the world clock is paused meanwhile).
    Record,
    /// The gateway's own handling of a model call, besides the provider's answer.
    Gateway,
    /// Gate checks: reading the document a gate asks for.
    Gate,
    /// Calls the world's links make after a step.
    Link,
    /// The MCP endpoint's own handling of a tool call, besides running it.
    Mcp,
    /// Waiting for the branch's lock, which keeps steps and saves in one order.
    Wait,
    /// Saving the final world and scoring it after the agents are done.
    Score,
    /// Someone paused the branch: nothing ran meanwhile.
    Pause,
}

/// Milliseconds per kind of Shakedown work; a kind with none is left out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Overhead {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub save: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub restore: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub record: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gateway: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gate: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub link: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub mcp: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub wait: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub score: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pause: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl Overhead {
    fn slot(&mut self, cat: Cat) -> &mut f64 {
        match cat {
            Cat::Save => &mut self.save,
            Cat::Restore => &mut self.restore,
            Cat::Record => &mut self.record,
            Cat::Gateway => &mut self.gateway,
            Cat::Gate => &mut self.gate,
            Cat::Link => &mut self.link,
            Cat::Mcp => &mut self.mcp,
            Cat::Wait => &mut self.wait,
            Cat::Score => &mut self.score,
            Cat::Pause => &mut self.pause,
        }
    }

    pub fn add(&mut self, cat: Cat, ms: f64) {
        *self.slot(cat) += ms.max(0.0);
    }

    /// Every kind of `other` added to this one.
    pub fn merge(&mut self, other: &Overhead) {
        for (cat, ms) in other.parts() {
            self.add(cat, ms);
        }
    }

    /// Each kind and its milliseconds.
    pub fn parts(&self) -> [(Cat, f64); 10] {
        [
            (Cat::Save, self.save),
            (Cat::Restore, self.restore),
            (Cat::Record, self.record),
            (Cat::Gateway, self.gateway),
            (Cat::Gate, self.gate),
            (Cat::Link, self.link),
            (Cat::Mcp, self.mcp),
            (Cat::Wait, self.wait),
            (Cat::Score, self.score),
            (Cat::Pause, self.pause),
        ]
    }

    pub fn total(&self) -> f64 {
        self.parts().iter().map(|(_, ms)| ms).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0.0
    }

    /// `ms` of kind `cat`; None when that is no time.
    pub fn of(cat: Cat, ms: f64) -> Option<Overhead> {
        let mut o = Overhead::default();
        o.add(cat, ms);
        (!o.is_empty()).then_some(o)
    }
}

/// Milliseconds since `since`, with the fraction kept.
pub fn ms_since(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// The overhead built up on one branch and not yet recorded.
#[derive(Debug, Default)]
pub struct Ledger {
    pending: Mutex<Overhead>,
}

impl Ledger {
    /// Adds `ms` of Shakedown work of kind `cat` to the branch's next recorded event.
    pub fn add(&self, cat: Cat, ms: f64) {
        self.pending.lock().unwrap().add(cat, ms);
    }

    /// Adds the time since `since`, of kind `cat`.
    pub fn add_since(&self, cat: Cat, since: Instant) {
        self.add(cat, ms_since(since));
    }

    /// Adds the time since `since` but the `besides_ms` of it spent on other work, of kind `cat`.
    pub fn add_besides(&self, cat: Cat, since: Instant, besides_ms: f64) {
        self.add(cat, ms_since(since) - besides_ms);
    }

    /// What built up since the last take; None when nothing did.
    pub fn take(&self) -> Option<Overhead> {
        let taken = std::mem::take(&mut *self.pending.lock().unwrap());
        (!taken.is_empty()).then_some(taken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ledger_hands_out_what_built_up_once() {
        let (l, other) = (Ledger::default(), Ledger::default());
        assert_eq!(l.take(), None);
        l.add(Cat::Save, 2.0);
        l.add(Cat::Save, 1.5);
        l.add(Cat::Record, 0.25);
        other.add(Cat::Mcp, 9.0);
        let o = l.take().unwrap();
        assert_eq!((o.save, o.record, o.total()), (3.5, 0.25, 3.75));
        assert_eq!(l.take(), None);
        assert_eq!(other.take().unwrap().mcp, 9.0);
    }

    #[test]
    fn only_the_kinds_with_time_are_written() {
        let mut o = Overhead::default();
        o.add(Cat::Gate, 4.0);
        o.add(Cat::Wait, -1.0);
        assert_eq!(
            serde_json::to_value(o).unwrap(),
            serde_json::json!({"gate": 4.0})
        );
        let mut sum = Overhead::default();
        sum.merge(&o);
        sum.merge(&o);
        assert_eq!(sum.gate, 8.0);
    }
}
