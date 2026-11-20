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

    /// Finding the saves no event of any org names (at start, and when asked on this machine).
    pub(crate) fn startup_gc() -> SystemScope {
        SystemScope {
            job: SystemJob::StartupGc,
        }
    }

    /// Finding the token a bearer secret's hash names (one fixed query).
    pub(crate) fn resolve_token() -> SystemScope {
        SystemScope {
            job: SystemJob::ResolveToken,
        }
    }

    /// Listing the orgs a user belongs to (`/api/me`).
    pub(crate) fn list_orgs_of_user() -> SystemScope {
        SystemScope {
            job: SystemJob::ListOrgsOfUser,
        }
    }

    /// Marking what was running when shaked stopped as interrupted.
    pub(crate) fn interrupt_at_start() -> SystemScope {
        SystemScope {
            job: SystemJob::InterruptAtStart,
        }
    }
}
