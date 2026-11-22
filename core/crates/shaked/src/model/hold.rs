//! What the control box last told a sandbox's worker about its branch (`branches.hold`): go on,
//! pause, or stop, the stop saying how the branch ends. The control box keeps it, enforces it on
//! the branch's model calls whatever the worker does, and has the last word on the status.

use super::BranchStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hold {
    #[default]
    Running,
    Paused,
    /// Stopping: the branch ends with this status (`stopped` or `budget_exceeded`), whatever its
    /// worker reports.
    Stopping(BranchStatus),
}

impl Hold {
    /// A stop pressed by a person.
    pub const STOPPED: Hold = Hold::Stopping(BranchStatus::Stopped);
    /// A stop for the run's spend limit.
    pub const OVER_BUDGET: Hold = Hold::Stopping(BranchStatus::BudgetExceeded);

    /// Its `branches.hold` value (None: running).
    pub fn stored(self) -> Option<&'static str> {
        match self {
            Hold::Running => None,
            Hold::Paused => Some("paused"),
            Hold::Stopping(BranchStatus::BudgetExceeded) => Some("budget_exceeded"),
            Hold::Stopping(_) => Some("stopping"),
        }
    }

    /// The hold a `branches.hold` value stands for.
    pub fn from_stored(v: Option<&str>) -> Hold {
        match v {
            Some("paused") => Hold::Paused,
            Some("budget_exceeded") => Hold::OVER_BUDGET,
            Some(_) => Hold::STOPPED,
            None => Hold::Running,
        }
    }

    /// The hold after `to` is asked for, when it changes: a stop is final (never turned into a
    /// pause or another stop), and going on clears only a pause.
    pub fn then(self, to: Hold) -> Option<Hold> {
        let changes = match (self, to) {
            (Hold::Stopping(_), _) => false,
            (Hold::Paused, Hold::Running) => true,
            (_, Hold::Running) => false,
            (now, to) => now != to,
        };
        changes.then_some(to)
    }

    /// The status the branch ends with, once it is stopping.
    pub fn ending(self) -> Option<BranchStatus> {
        match self {
            Hold::Stopping(status) => Some(status),
            Hold::Running | Hold::Paused => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_is_stored_and_read_back() {
        for h in [
            Hold::Running,
            Hold::Paused,
            Hold::STOPPED,
            Hold::OVER_BUDGET,
        ] {
            assert_eq!(Hold::from_stored(h.stored()), h);
        }
        assert_eq!(
            Hold::OVER_BUDGET.ending(),
            Some(BranchStatus::BudgetExceeded)
        );
        assert_eq!(Hold::Paused.ending(), None);
        assert_eq!(Hold::Running.then(Hold::Paused), Some(Hold::Paused));
        assert_eq!(Hold::Paused.then(Hold::Running), Some(Hold::Running));
        assert_eq!(Hold::Running.then(Hold::Running), None);
        assert_eq!(
            Hold::Paused.then(Hold::OVER_BUDGET),
            Some(Hold::OVER_BUDGET)
        );
        assert_eq!(Hold::STOPPED.then(Hold::Running), None);
        assert_eq!(Hold::STOPPED.then(Hold::OVER_BUDGET), None);
    }
}
