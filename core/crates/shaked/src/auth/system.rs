//! The background jobs that run without a caller: their transactions are the layer's to set up
//! (a hosting layer runs them on its system login, where every owner's rows are visible). Each job
//! builds its own [`SystemScope`] here and nowhere else, so only shaked's own jobs open one.

/// A job that runs without a caller (what the layer's `begin` is told).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemJob {
    /// Marking what was running when shaked stopped as interrupted.
    InterruptAtStart,
    /// Finding the saves no event names (at start, and when asked on this machine).
    StartupGc,
    /// Finding the token a bearer secret's hash names (one fixed query).
    ResolveToken,
    /// Copying stored objects between stores.
    StoreCopy,
    /// Ending the branches sandboxes' workers run that the control box stops, or whose worker
    /// token died (at start, and now and then), and finding which branches the control box's
    /// sandboxes run still run (the sandbox sweep).
    LinkSweep,
    /// Sending ended branches to their observability targets, and recording how it went.
    ObservabilityExport,
}

/// Permission to open one system job's transactions (`Store::system_tx`). Built only in this
/// crate, by the job it names.
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

    /// Finding the saves no event of any owner names.
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

    /// Listing and repointing saves between stores.
    pub(crate) fn store_copy() -> SystemScope {
        SystemScope {
            job: SystemJob::StoreCopy,
        }
    }

    /// Finding and ending linked branches the control box stops, or whose worker token died.
    pub(crate) fn link_sweep() -> SystemScope {
        SystemScope {
            job: SystemJob::LinkSweep,
        }
    }

    /// Sending ended branches to the observability targets their owners queued them for.
    pub(crate) fn observability_export() -> SystemScope {
        SystemScope {
            job: SystemJob::ObservabilityExport,
        }
    }
}
