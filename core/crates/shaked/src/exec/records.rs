//! Where a branch writes and reads its own events and status: one trait, so a branch can record
//! into the store (here) or over a link to another process.

use std::sync::Arc;

use futures_util::future::BoxFuture;
use serde_json::Value;

use crate::{
    auth::Caller,
    model::*,
    overhead::Overhead,
    store::{BranchId, Store},
};

/// A branch's records: the events it appends and reads back, and its status.
pub trait BranchRecords: Send + Sync {
    /// Appends an event of the branch; an object payload also gets `overhead` as `overhead_ms`.
    fn append<'a>(
        &'a self,
        agent: Option<&'a str>,
        kind: EventKind,
        payload: Value,
        overhead: Option<Overhead>,
    ) -> BoxFuture<'a, anyhow::Result<Event>>;

    /// The branch's events in order (its fork parents' events included, as the store has them).
    fn events(&self) -> BoxFuture<'_, anyhow::Result<Vec<Event>>>;

    /// Sets the branch's status (and its error, when it ended in one).
    fn set_status(
        &self,
        status: BranchStatus,
        error: Option<String>,
    ) -> BoxFuture<'_, anyhow::Result<()>>;
}

/// A branch's records in the store, written for `caller`.
pub struct StoreRecords {
    store: Arc<Store>,
    caller: Arc<Caller>,
    branch_id: BranchId,
}

impl StoreRecords {
    pub fn new(store: Arc<Store>, caller: Arc<Caller>, branch_id: BranchId) -> Self {
        StoreRecords {
            store,
            caller,
            branch_id,
        }
    }
}

impl BranchRecords for StoreRecords {
    fn append<'a>(
        &'a self,
        agent: Option<&'a str>,
        kind: EventKind,
        payload: Value,
        overhead: Option<Overhead>,
    ) -> BoxFuture<'a, anyhow::Result<Event>> {
        Box::pin(async move {
            let at = self.store.at(&self.caller);
            at.append_with(&self.branch_id, agent, kind, payload, overhead)
                .await
        })
    }

    fn events(&self) -> BoxFuture<'_, anyhow::Result<Vec<Event>>> {
        Box::pin(async move { self.store.at(&self.caller).events(&self.branch_id).await })
    }

    fn set_status(
        &self,
        status: BranchStatus,
        error: Option<String>,
    ) -> BoxFuture<'_, anyhow::Result<()>> {
        Box::pin(async move {
            let at = self.store.at(&self.caller);
            at.set_status(&self.branch_id, status, error).await
        })
    }
}

/// The records of a branch that has none set: it refuses everything (a test helper's mistake,
/// never production's).
pub(super) struct NoRecords;

impl BranchRecords for NoRecords {
    fn append<'a>(
        &'a self,
        _agent: Option<&'a str>,
        _kind: EventKind,
        _payload: Value,
        _overhead: Option<Overhead>,
    ) -> BoxFuture<'a, anyhow::Result<Event>> {
        Box::pin(async { Err(anyhow::anyhow!("no records")) })
    }

    fn events(&self) -> BoxFuture<'_, anyhow::Result<Vec<Event>>> {
        Box::pin(async { Err(anyhow::anyhow!("no records")) })
    }

    fn set_status(
        &self,
        _status: BranchStatus,
        _error: Option<String>,
    ) -> BoxFuture<'_, anyhow::Result<()>> {
        Box::pin(async { Err(anyhow::anyhow!("no records")) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_branch_with_no_records_refuses_to_append() {
        let err = NoRecords
            .append(None, EventKind::ToolCall, Value::Null, None)
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "no records");
    }
}
