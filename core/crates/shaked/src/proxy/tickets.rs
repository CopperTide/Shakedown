//! Proxy tickets: the random secret in a proxy URL, and the branch (and agent) it stands for.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use base64::Engine;

/// Who a ticket stands for: a branch, and the agent when the ticket is an agent's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    pub branch: String,
    pub agent: Option<String>,
}

/// The tickets shaked gave out.
#[derive(Debug, Default)]
pub struct Tickets {
    held: Mutex<HashMap<String, Held>>,
}

#[derive(Debug)]
struct Held {
    holder: Holder,
    expires: Instant,
}

impl Tickets {
    /// A new ticket for `agent` (None: the branch's driver) on `branch`, good until `expires`.
    pub fn issue(&self, branch: &str, agent: Option<&str>, expires: Instant) -> String {
        let ticket = uuid::Uuid::new_v4().simple().to_string();
        let holder = Holder {
            branch: branch.to_string(),
            agent: agent.map(str::to_string),
        };
        self.held
            .lock()
            .unwrap()
            .insert(ticket.clone(), Held { holder, expires });
        ticket
    }

    /// Who holds `ticket`, while it is good.
    pub fn holder(&self, ticket: &str) -> Option<Holder> {
        let held = self.held.lock().unwrap();
        held.get(ticket)
            .filter(|h| h.expires > Instant::now())
            .map(|h| h.holder.clone())
    }

    /// Takes back the tickets of `agent` on `branch` (every one of the branch when None).
    pub fn revoke(&self, branch: &str, agent: Option<&str>) {
        let mine = |h: &Held| {
            h.holder.branch == branch && (agent.is_none() || h.holder.agent.as_deref() == agent)
        };
        self.held.lock().unwrap().retain(|_, h| !mine(h));
    }

    /// Moves `branch`'s tickets' expiry `by` later (the branch was paused that long).
    pub fn extend(&self, branch: &str, by: Duration) {
        for h in self
            .held
            .lock()
            .unwrap()
            .values_mut()
            .filter(|h| h.holder.branch == branch)
        {
            h.expires += by;
        }
    }
}

pub const PROXY_USER: &str = "shakedown";
/// The user name until 2026-11-19, still taken (a client may hold a URL from before).
pub const OLD_PROXY_USER: &str = "arena";

/// The ticket in a `Proxy-Authorization` header (`Basic base64("shakedown:<ticket>")`, or the old
/// user `arena`); None for another scheme or user.
pub fn ticket_of(proxy_authorization: Option<&str>) -> Option<String> {
    let (scheme, encoded) = proxy_authorization?.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let engine = base64::engine::general_purpose::STANDARD;
    let decoded = String::from_utf8(engine.decode(encoded.trim()).ok()?).ok()?;
    let (user, ticket) = decoded.split_once(':')?;
    let ours = user == PROXY_USER || user == OLD_PROXY_USER;
    (ours && !ticket.is_empty()).then(|| ticket.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ticket_is_read_from_basic_auth_of_the_shakedown_user_or_the_old_arena_one() {
        // `shakedown:t1`, and the old user `arena:t1`.
        assert_eq!(
            ticket_of(Some("Basic c2hha2Vkb3duOnQx")).as_deref(),
            Some("t1")
        );
        assert_eq!(ticket_of(Some("Basic YXJlbmE6dDE=")).as_deref(), Some("t1"));
        assert_eq!(ticket_of(Some("basic YXJlbmE6dDE=")).as_deref(), Some("t1"));
        // "other:t1"
        assert_eq!(ticket_of(Some("Basic b3RoZXI6dDE=")), None);
        assert_eq!(ticket_of(Some("Bearer YXJlbmE6dDE=")), None);
        assert_eq!(ticket_of(Some("Basic !!")), None);
        assert_eq!(ticket_of(None), None);
    }

    #[test]
    fn a_ticket_stands_for_its_branch_and_agent() {
        let t = Tickets::default();
        let later = Instant::now() + Duration::from_secs(60);
        let a = t.issue("b1", Some("planner"), later);
        let d = t.issue("b1", None, later);
        assert_ne!(a, d);
        assert_eq!(t.holder(&a).unwrap().agent.as_deref(), Some("planner"));
        assert_eq!(
            t.holder(&d).unwrap(),
            Holder {
                branch: "b1".into(),
                agent: None
            }
        );
        assert!(t.holder("nope").is_none());
    }

    #[test]
    fn an_expired_ticket_is_refused_and_a_pause_extends_it() {
        let t = Tickets::default();
        let soon = Instant::now() + Duration::from_millis(50);
        let a = t.issue("b1", Some("a"), soon);
        t.extend("b1", Duration::from_millis(200));
        std::thread::sleep(Duration::from_millis(100));
        assert!(t.holder(&a).is_some(), "paused time does not count");
        std::thread::sleep(Duration::from_millis(200));
        assert!(t.holder(&a).is_none(), "expired");
        let b = t.issue("b1", Some("b"), Instant::now() + Duration::from_secs(60));
        t.revoke("b1", Some("b"));
        assert!(t.holder(&b).is_none(), "revoked with its agent");
    }
}
