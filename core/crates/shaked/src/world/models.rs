//! The model endpoint a branch's world gets for its own model calls: a simulated person a part
//! plays, or a judge the benchmark's scoring asks. Each part's instances, and the scoring, call
//! `{gateway}/world.<who>/v1` with the branch's one world token; the gateway records each call
//! under the actor `world.<who>` (`world.<who>.<name>` when the call names who speaks).

use parts::proto::ModelsParams;

use crate::auth::tokens::{hash, new_secret};

/// What every world actor's id starts with. Team agent ids never hold a dot, so none collides.
pub const WORLD_ACTOR_PREFIX: &str = "world.";
/// The actor of the benchmark's scoring (`world.scoring`).
pub const SCORING: &str = "scoring";
const TOKEN_PREFIX: &str = "sdw_";

/// A branch's world endpoint: its gateway base (`{shaked}/gateway/{branch}`) and its token.
#[derive(Clone)]
pub struct WorldModels {
    gateway: String,
    token: String,
}

impl std::fmt::Debug for WorldModels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldModels")
            .field("gateway", &self.gateway)
            .finish_non_exhaustive()
    }
}

impl WorldModels {
    /// A new endpoint for `branch` on the shaked at `base`, with a new random token.
    pub fn new(base: &str, branch: &str) -> WorldModels {
        let base = base.trim_end_matches('/');
        WorldModels {
            gateway: format!("{base}/gateway/{branch}"),
            token: new_secret(TOKEN_PREFIX),
        }
    }

    /// A new endpoint for `branch` on this shaked, where its parts reach it: its own address
    /// (`self_base`), else its public one.
    pub fn for_branch(config: &crate::app::Config, branch: &str) -> WorldModels {
        let own = config.self_base.trim();
        let base = if own.is_empty() {
            config.public_base()
        } else {
            own
        };
        WorldModels::new(base, branch)
    }

    /// What `who` (a part, or [`SCORING`]) is given: its base URL and the token.
    pub fn of(&self, who: &str) -> ModelsParams {
        ModelsParams {
            base_url: format!("{}/{WORLD_ACTOR_PREFIX}{who}/v1", self.gateway),
            token: self.token.clone(),
        }
    }

    /// True when `authorization` (the header's value) is `Bearer <this token>`.
    pub fn token_ok(&self, authorization: Option<&str>) -> bool {
        let given = authorization.and_then(|a| a.strip_prefix("Bearer "));
        // Digests compared, so the time taken says nothing about the token.
        given.is_some_and(|g| hash(g.trim()) == hash(&self.token))
    }
}

/// True for a world actor's id (`world.<who>` or `world.<who>.<name>`).
pub fn is_world_actor(agent: Option<&str>) -> bool {
    agent.is_some_and(|a| a.starts_with(WORLD_ACTOR_PREFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_part_and_the_scoring_get_their_own_url_and_the_one_token() {
        let m = WorldModels::new("http://127.0.0.1:4510/", "b1");
        let shop = m.of("shop");
        assert_eq!(
            shop.base_url,
            "http://127.0.0.1:4510/gateway/b1/world.shop/v1"
        );
        assert_eq!(
            m.of(SCORING).base_url,
            "http://127.0.0.1:4510/gateway/b1/world.scoring/v1"
        );
        assert!(shop.token.starts_with("sdw_") && shop.token == m.of(SCORING).token);
        assert!(m.token_ok(Some(&format!("Bearer {}", shop.token))));
        assert!(!m.token_ok(Some("Bearer sdw_other")) && !m.token_ok(None));
        let other = WorldModels::new("http://h", "b1");
        assert_ne!(
            other.of("shop").token,
            shop.token,
            "each branch has its own token"
        );
        assert!(
            !format!("{m:?}").contains(&shop.token),
            "the token is never printed"
        );
    }

    #[test]
    fn world_actors_start_with_world_and_a_dot() {
        assert!(is_world_actor(Some("world.shop")) && is_world_actor(Some("world.shop.customer")));
        assert!(!is_world_actor(Some("world")) && !is_world_actor(Some("agent")));
        assert!(!is_world_actor(None));
    }
}
