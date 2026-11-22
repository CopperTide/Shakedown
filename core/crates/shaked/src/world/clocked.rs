//! Moving a world's clock parts (`uses_clock`) to a time with `advance`, remembering which ones
//! changed (to save them after the next step) and when each next has something scheduled
//! (`wake_at_ms`), and collecting the events they report.

use parts::proto::{AdvanceParams, Advanced, WorldEvent};

use super::{request, Part, World, WorldError};

/// An event a part reported while its clock advanced.
#[derive(Debug, Clone, PartialEq)]
pub struct PartEvent {
    pub part: String,
    pub event: WorldEvent,
    pub clock_ms: i64,
}

async fn advance(p: &Part, clock_ms: i64) -> Result<Advanced, WorldError> {
    let params = AdvanceParams {
        instance: p.instance.clone(),
        clock_ms,
    };
    request(&p.client, &p.label(), &params).await
}

impl World {
    /// Advances the clock part `p` to `clock_ms`, remembering when it changed.
    async fn advance_tracked(&self, p: &Part, clock_ms: i64) -> Result<Advanced, WorldError> {
        let a = advance(p, clock_ms).await?;
        let mut track = self.track.lock().unwrap();
        if a.changed {
            track.changed.insert(p.name.clone());
        }
        match a.wake_at_ms {
            Some(at) => track.wakes.insert(p.name.clone(), at),
            None => track.wakes.remove(&p.name),
        };
        Ok(a)
    }

    /// Advances every clock part to `clock_ms`: the events they reported, in world order, and the
    /// earliest clock any part has something scheduled for after `clock_ms`.
    pub async fn advance_all(
        &self,
        clock_ms: i64,
    ) -> Result<(Vec<PartEvent>, Option<i64>), WorldError> {
        let mut out = vec![];
        for p in self.parts.iter().filter(|p| p.describe.uses_clock) {
            let a = self.advance_tracked(p, clock_ms).await?;
            out.extend(a.events.into_iter().map(|event| PartEvent {
                part: p.name.clone(),
                event,
                clock_ms,
            }));
        }
        let mut track = self.track.lock().unwrap();
        track.advanced_to = track.advanced_to.max(clock_ms);
        track.advanced = true;
        drop(track);
        Ok((out, self.next_wake(clock_ms)))
    }

    /// The latest clock every clock part was advanced to together (None before any advance).
    pub fn advanced_to(&self) -> Option<i64> {
        let track = self.track.lock().unwrap();
        track.advanced.then_some(track.advanced_to)
    }

    /// The earliest clock after `after_ms` that a part said it has something scheduled for.
    pub fn next_wake(&self, after_ms: i64) -> Option<i64> {
        let track = self.track.lock().unwrap();
        track
            .wakes
            .values()
            .copied()
            .filter(|w| *w > after_ms)
            .min()
    }

    /// The earliest clock a part said it has something scheduled for, after the clock the world
    /// was last advanced to and at or before `by_ms`: what is due by then (a stepped clock
    /// charged past it) and not yet advanced to.
    pub fn due_by(&self, by_ms: i64) -> Option<i64> {
        let track = self.track.lock().unwrap();
        let after = track.advanced_to;
        let due = track.wakes.values().copied();
        due.filter(|w| *w > after && *w <= by_ms).min()
    }

    /// Notes the wake a call's result gave for `part` (none: unchanged).
    pub(super) fn called_wake(&self, part: &str, wake_at_ms: Option<i64>) {
        if let Some(at) = wake_at_ms {
            self.track
                .lock()
                .unwrap()
                .wakes
                .insert(part.to_string(), at);
        }
    }

    /// Advances `part` to `clock_ms` when it uses the clock, dropping the events it reports (for
    /// a rerun's start: they were recorded the first time).
    pub async fn advance_part(&self, part: &str, clock_ms: i64) -> Result<(), WorldError> {
        let p = self.part(part)?;
        if p.describe.uses_clock {
            self.advance_tracked(p, clock_ms).await?;
        }
        Ok(())
    }
}
