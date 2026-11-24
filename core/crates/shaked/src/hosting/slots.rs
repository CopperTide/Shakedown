//! How many sandboxes a scope (the layer's `object_prefix`) runs at a time: at most
//! [`PER_SCOPE`], counted on this control box. A
//! run beyond that is refused before anything is recorded; its slot is given back when its
//! sandbox has been killed.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::StatusCode;

use crate::layer::Refusal;

/// Sandboxes a scope may run at once.
pub const PER_SCOPE: usize = 1;

/// Why a run beyond [`PER_ORG`] is refused.
pub const BUSY: &str =
    "a cloud run of yours is already going (one at a time for now): start this one when it ends";

/// The sandboxes each scope runs, by scope.
#[derive(Debug, Default)]
pub(super) struct Slots(Arc<Mutex<HashMap<String, usize>>>);

/// One sandbox of `scope` counted, until dropped.
#[derive(Debug)]
pub struct Slot {
    taken: Arc<Mutex<HashMap<String, usize>>>,
    scope: String,
}

impl Slots {
    /// A slot for `scope`, refused 429 ([`BUSY`]) when it runs [`PER_SCOPE`] already.
    pub(super) fn take(&self, scope: &str) -> Result<Slot, Refusal> {
        let mut counts = self.0.lock().unwrap();
        let n = counts.entry(scope.to_string()).or_default();
        if *n >= PER_SCOPE {
            return Err(Refusal::new(StatusCode::TOO_MANY_REQUESTS, BUSY));
        }
        *n += 1;
        Ok(Slot {
            taken: self.0.clone(),
            scope: scope.to_string(),
        })
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut taken = self.taken.lock().unwrap();
        if let Some(n) = taken.get_mut(&self.scope) {
            *n -= 1;
            if *n == 0 {
                taken.remove(&self.scope);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_gets_one_slot_until_it_is_given_back() {
        let slots = Slots::default();
        let a = slots.take("a").unwrap();
        assert_eq!(slots.take("a").unwrap_err().message, BUSY);
        let b = slots.take("b").unwrap();
        drop(a);
        assert!(slots.take("a").is_ok());
        drop(b);
    }
}
