//! System scopes: the background jobs that may read and write every org's rows (on the system
//! pool, as `shaked_system`, which bypasses row-level security). Each job builds its own scope
//! here and nowhere else.

/// The jobs that span orgs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemJob {
    InterruptAtStart,
    StartupGc,
    ResolveToken,
    ListOrgsOfUser,
    StoreCopy,
}

/// Permission to run one system job's transactions.
#[derive(Debug, Clone, Copy)]
pub struct SystemScope {
    job: SystemJob,
}

impl SystemScope {
    pub fn job(&self) -> SystemJob {
        self.job
    }

    /// Marking what was running when shaked stopped as interrupted.
    pub(crate) fn interrupt_at_start() -> SystemScope {
        SystemScope {
            job: SystemJob::InterruptAtStart,
        }
    }
}
