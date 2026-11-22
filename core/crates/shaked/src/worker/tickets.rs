//! One-use tickets a sandbox's worker trades for its branch's worker token. A ticket is bound to
//! one branch and one sandbox, lives ten minutes and is kept in this control box's memory only,
//! as the sha256 of its secret; a redeemed ticket is compared with every held one in constant
//! time.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    auth::{run_tokens::prefix, tokens::new_secret, Caller, RunTokenKind},
    store::BranchId,
};

/// How long a ticket may wait to be redeemed.
pub const TICKET_TTL: Duration = Duration::from_secs(600);

/// What a ticket is for: the branch a sandbox runs, and who the branch records for.
#[derive(Debug, Clone)]
pub struct Ticket {
    pub branch_id: BranchId,
    /// The sandbox's id (a uuid: the worker token names it).
    pub sandbox: String,
    pub caller: Arc<Caller>,
}

struct Held {
    digest: [u8; 32],
    ticket: Ticket,
    until: Instant,
}

/// The tickets issued and not yet redeemed.
#[derive(Default)]
pub struct Tickets(Mutex<Vec<Held>>);

fn digest(secret: &str) -> [u8; 32] {
    let d = ring::digest::digest(&ring::digest::SHA256, secret.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(d.as_ref());
    out
}

/// True when `a` and `b` are equal, in time that does not depend on where they differ.
fn same(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let diff = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(diff) == 0
}

impl Tickets {
    /// A new ticket for `t`: its secret, good once within [`TICKET_TTL`].
    pub fn issue(&self, t: Ticket) -> String {
        self.issue_lasting(t, TICKET_TTL)
    }

    /// `issue` with a ticket good for `ttl` (tests make expired ones with zero).
    pub fn issue_lasting(&self, t: Ticket, ttl: Duration) -> String {
        let secret = new_secret(prefix(RunTokenKind::Ticket));
        let now = Instant::now();
        let mut held = self.0.lock().unwrap();
        held.retain(|h| h.until > now);
        held.push(Held {
            digest: digest(&secret),
            ticket: t,
            until: now + ttl,
        });
        secret
    }

    /// The ticket `secret` names, used up by this call; None when it is unknown, used or expired.
    pub fn redeem(&self, secret: &str) -> Option<Ticket> {
        let asked = digest(secret);
        let mut held = self.0.lock().unwrap();
        let mut found = None;
        for (i, h) in held.iter().enumerate() {
            if same(&h.digest, &asked) {
                found = Some(i);
            }
        }
        let h = held.swap_remove(found?);
        (h.until > Instant::now()).then_some(h.ticket)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket(branch: &str) -> Ticket {
        Ticket {
            branch_id: branch.into(),
            sandbox: uuid::Uuid::new_v4().to_string(),
            caller: Arc::new(Caller::person()),
        }
    }

    #[test]
    fn a_ticket_redeems_once_for_its_branch() {
        let tickets = Tickets::default();
        let a = tickets.issue(ticket("a"));
        let b = tickets.issue(ticket("b"));
        assert!(a.starts_with("sdt_") && a != b);
        assert_eq!(tickets.redeem(&b).unwrap().branch_id, "b");
        assert_eq!(tickets.redeem(&a).unwrap().branch_id, "a");
        assert!(tickets.redeem(&a).is_none());
        assert!(tickets.redeem("sdt_nope").is_none());
    }

    #[test]
    fn an_expired_ticket_is_refused() {
        let tickets = Tickets::default();
        let t = tickets.issue_lasting(ticket("a"), Duration::ZERO);
        assert!(tickets.redeem(&t).is_none());
    }

    #[test]
    fn equal_digests_only_are_the_same() {
        let (a, b) = (digest("x"), digest("y"));
        assert!(same(&a, &a) && !same(&a, &b));
    }
}
