//! The background jobs that run without a caller: their transactions are the layer's to set up
//! (a hosting layer runs them where every owner's rows are visible).

/// A job that runs without a caller.
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
}
