//! A fixed-step clock: the way to meet the clock contract (the state after `advance` does not depend on
//! how the advances were split). Run the world one whole step at a time; a remainder waits for the next
//! advance.

/// How far a world has run, in whole steps of `step_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedStep {
    step_ms: u64,
    done_ms: u64,
}

impl FixedStep {
    /// A clock at 0. Panics when `step_ms` is 0.
    pub fn new(step_ms: u64) -> FixedStep {
        assert!(step_ms > 0, "step_ms must be positive");
        FixedStep {
            step_ms,
            done_ms: 0,
        }
    }

    /// A clock at the last whole step at or before `clock_ms`.
    pub fn starting_at(step_ms: u64, clock_ms: u64) -> FixedStep {
        let mut clock = FixedStep::new(step_ms);
        clock.done_ms = clock_ms - clock_ms % step_ms;
        clock
    }

    pub fn step_ms(&self) -> u64 {
        self.step_ms
    }

    /// How far the world has run.
    pub fn done_ms(&self) -> u64 {
        self.done_ms
    }

    /// Where a restored world had run to.
    pub fn set_done_ms(&mut self, done_ms: u64) {
        self.done_ms = done_ms;
    }

    /// Calls `step(step_ms)` once per whole step up to `clock_ms`; the remainder waits for the next
    /// call. How many steps ran.
    pub fn advance_to(&mut self, clock_ms: u64, mut step: impl FnMut(u64)) -> u64 {
        let mut n = 0;
        while self.done_ms + self.step_ms <= clock_ms {
            self.done_ms += self.step_ms;
            step(self.step_ms);
            n += 1;
        }
        n
    }
}
