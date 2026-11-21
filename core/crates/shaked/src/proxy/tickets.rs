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

/// The tickets shaked gave out, and those of branches that ended (kept [`ENDED_KEPT`], so a
/// client still holding one is told its branch ended rather than asked for a ticket).
#[derive(Debug, Default)]
pub struct Tickets {
    held: Mutex<HashMap<String, Held>>,
    ended: Mutex<HashMap<String, Instant>>,
}

/// How long a taken-back ticket is remembered as ended.
pub const ENDED_KEPT: Duration = Duration::from_secs(3600);

/// What a ticket stands for now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Good: its holder.
    Good(Holder),
    /// Its branch ended (or its agent's tickets were taken back) within [`ENDED_KEPT`].
    Ended,
    /// Never given, expired, or ended long ago.
    Unknown,
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

    /// How many tickets are held (expired ones too, until their branch ends).
    pub fn len(&self) -> usize {
        self.held.lock().unwrap().len()
    }

    /// True when no ticket is held.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Who holds `ticket`, while it is good.
    pub fn holder(&self, ticket: &str) -> Option<Holder> {
        match self.standing(ticket) {
            Standing::Good(h) => Some(h),
            _ => None,
        }
    }

    /// What `ticket` stands for now.
    pub fn standing(&self, ticket: &str) -> Standing {
        let good = (self.held.lock().unwrap().get(ticket))
            .filter(|h| h.expires > Instant::now())
            .map(|h| h.holder.clone());
        if let Some(h) = good {
            return Standing::Good(h);
        }
        let ended = self.ended.lock().unwrap().get(ticket).copied();
        match ended.is_some_and(|at| at.elapsed() < ENDED_KEPT) {
            true => Standing::Ended,
            false => Standing::Unknown,
        }
    }

    /// Takes back the tickets of `agent` on `branch` (every one of the branch when None); they
    /// are remembered as ended for [`ENDED_KEPT`].
    pub fn revoke(&self, branch: &str, agent: Option<&str>) {
        let mine = |h: &Held| {
            h.holder.branch == branch && (agent.is_none() || h.holder.agent.as_deref() == agent)
        };
        let mut taken = vec![];
        self.held.lock().unwrap().retain(|t, h| match mine(h) {
            true => {
                taken.push(t.clone());
                false
            }
            false => true,
        });
        let now = Instant::now();
        let mut ended = self.ended.lock().unwrap();
        ended.retain(|_, at| at.elapsed() < ENDED_KEPT);
        ended.extend(taken.into_iter().map(|t| (t, now)));
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

/// The ticket in a `Proxy-Authorization` header (`Basic base64("shakedown:<ticket>")`); None for
/// another scheme or user.
pub fn ticket_of(proxy_authorization: Option<&str>) -> Option<String> {
    let (scheme, encoded) = proxy_authorization?.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let engine = base64::engine::general_purpose::STANDARD;
    let decoded = String::from_utf8(engine.decode(encoded.trim()).ok()?).ok()?;
    let (user, ticket) = decoded.split_once(':')?;
    let ours = user == PROXY_USER;
    (ours && !ticket.is_empty()).then(|| ticket.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ticket_is_read_from_basic_auth_of_the_shakedown_user() {
        // `shakedown:t1`
        assert_eq!(
            ticket_of(Some("Basic c2hha2Vkb3duOnQx")).as_deref(),
            Some("t1")
        );
        assert_eq!(
            ticket_of(Some("basic c2hha2Vkb3duOnQx")).as_deref(),
            Some("t1")
        );
        // "other:t1"
        assert_eq!(ticket_of(Some("Basic b3RoZXI6dDE=")), None);
        assert_eq!(ticket_of(Some("Bearer c2hha2Vkb3duOnQx")), None);
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

    /// M2: a ticket of a branch that ended is known as ended (its proxy says so), not unknown.
    #[test]
    fn an_ended_branch_s_tickets_are_remembered_as_ended() {
        let t = Tickets::default();
        let a = t.issue("b1", Some("a"), Instant::now() + Duration::from_secs(60));
        let other = t.issue("b2", None, Instant::now() + Duration::from_secs(60));
        t.revoke("b1", None);
        assert_eq!(t.standing(&a), Standing::Ended);
        assert!(matches!(t.standing(&other), Standing::Good(_)));
        assert_eq!(t.standing("never-given"), Standing::Unknown);
        assert_eq!(t.len(), 1, "an ended ticket is not held");
    }
}
