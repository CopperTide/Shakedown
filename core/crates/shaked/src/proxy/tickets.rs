//! Proxy tickets: the random secret in a proxy URL, and the branch (and agent) it stands for.

use std::{collections::HashMap, sync::Mutex};

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
    held: Mutex<HashMap<String, Holder>>,
}

impl Tickets {
    /// A new ticket (32 hex digits) for `agent` on `branch`.
    pub fn issue(&self, branch: &str, agent: Option<&str>) -> String {
        let ticket = uuid::Uuid::new_v4().simple().to_string();
        let holder = Holder {
            branch: branch.to_string(),
            agent: agent.map(str::to_string),
        };
        self.held.lock().unwrap().insert(ticket.clone(), holder);
        ticket
    }

    /// Who `ticket` stands for; None for a ticket shaked never gave.
    pub fn holder(&self, ticket: &str) -> Option<Holder> {
        self.held.lock().unwrap().get(ticket).cloned()
    }
}

/// The user name of every proxy URL shaked gives.
pub const PROXY_USER: &str = "arena";

/// The ticket in a `Proxy-Authorization` header (`Basic base64("arena:<ticket>")`); None for
/// another scheme or user.
pub fn ticket_of(proxy_authorization: Option<&str>) -> Option<String> {
    let (scheme, encoded) = proxy_authorization?.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let engine = base64::engine::general_purpose::STANDARD;
    let decoded = String::from_utf8(engine.decode(encoded.trim()).ok()?).ok()?;
    let (user, ticket) = decoded.split_once(':')?;
    (user == PROXY_USER && !ticket.is_empty()).then(|| ticket.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ticket_is_read_from_basic_auth_of_the_arena_user() {
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
        let a = t.issue("b1", Some("planner"));
        let d = t.issue("b1", None);
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
}
