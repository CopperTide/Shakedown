//! The agent tokens of a running branch (`sda_…`), kept in memory only: the plain token, to hand
//! to the agent, and when it expires. Expiry counts only unpaused time (`extend` adds a pause).

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::auth::tokens::{hash, new_secret};

pub const AGENT_PREFIX: &str = "sda_";

struct Held {
    token: String,
    expires: Instant,
    revoked: bool,
}

/// agent id -> its token.
#[derive(Default)]
pub struct AgentTokens {
    held: Mutex<HashMap<String, Held>>,
}

impl AgentTokens {
    /// A new token for `agent`, valid for `ttl`; it replaces any earlier one.
    pub fn issue(&self, agent: &str, ttl: Duration) -> String {
        let token = new_secret(AGENT_PREFIX);
        let held = Held {
            token: token.clone(),
            expires: Instant::now() + ttl,
            revoked: false,
        };
        self.held.lock().unwrap().insert(agent.to_string(), held);
        token
    }

    /// `agent`'s token while it is good, else a new one (valid for `ttl`).
    pub fn ensure(&self, agent: &str, ttl: Duration) -> String {
        match self.token_of(agent) {
            Some(t) => t,
            None => self.issue(agent, ttl),
        }
    }

    /// `agent`'s token while it is good.
    pub fn token_of(&self, agent: &str) -> Option<String> {
        let held = self.held.lock().unwrap();
        held.get(agent)
            .filter(|h| !h.revoked && h.expires > Instant::now())
            .map(|h| h.token.clone())
    }

    pub fn revoke(&self, agent: &str) {
        if let Some(h) = self.held.lock().unwrap().get_mut(agent) {
            h.revoked = true;
        }
    }

    pub fn revoke_all(&self) {
        for h in self.held.lock().unwrap().values_mut() {
            h.revoked = true;
        }
    }

    /// Moves every expiry `by` later (the branch was paused that long).
    pub fn extend(&self, by: Duration) {
        for h in self.held.lock().unwrap().values_mut() {
            h.expires += by;
        }
    }

    /// True when `authorization` (the header's value) is `Bearer <agent's good token>`; false for an
    /// agent never given one, revoked, expired or a mismatch.
    pub fn ok(&self, agent: &str, authorization: Option<&str>) -> bool {
        let given = authorization
            .and_then(|a| a.strip_prefix("Bearer "))
            .map(str::trim);
        let good = self.token_of(agent);
        match (given, good) {
            // Digests compared, so the time taken says nothing about the token.
            (Some(g), Some(t)) => hash(g) == hash(&t),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG: Duration = Duration::from_secs(3600);

    #[test]
    fn an_agent_without_a_token_or_with_another_gets_nothing() {
        let t = AgentTokens::default();
        assert!(!t.ok("a", None));
        assert!(!t.ok("a", Some("Bearer x")));
        let tok = t.issue("a", LONG);
        assert!(tok.starts_with("sda_"));
        assert!(t.ok("a", Some(&format!("Bearer {tok}"))));
        assert!(!t.ok("a", Some("Bearer sda_other")));
        assert!(
            !t.ok("b", Some(&format!("Bearer {tok}"))),
            "one agent's token is not another's"
        );
        t.revoke("a");
        assert!(!t.ok("a", Some(&format!("Bearer {tok}"))));
    }

    #[test]
    fn a_long_pause_does_not_expire_agent_tokens() {
        let t = AgentTokens::default();
        let tok = t.issue("a", Duration::from_millis(200));
        std::thread::sleep(Duration::from_millis(100));
        t.extend(Duration::from_millis(400)); // the branch was paused 400 ms
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            t.ok("a", Some(&format!("Bearer {tok}"))),
            "100 ms of the 200 ran"
        );
        std::thread::sleep(Duration::from_millis(250));
        assert!(
            !t.ok("a", Some(&format!("Bearer {tok}"))),
            "expired after its unpaused 200 ms"
        );
    }
}
