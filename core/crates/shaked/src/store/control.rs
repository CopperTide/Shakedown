//! What the control box decided for a branch, kept where a sandbox's worker cannot change it and
//! a restart does not lose it: the run rules it planned the branch with (`branches.rules`, its
//! spend limit among them) and what it last told the branch's worker (`branches.hold`).

use anyhow::Result;

use super::Scoped;
use crate::model::{Hold, Rules};

/// What the control box keeps of a linked branch, to rebuild its link after a restart.
#[derive(Debug, Clone, PartialEq)]
pub struct Controlled {
    /// The branch's spend limit (None: no limit) when its rules are on record; None when they
    /// are not (a branch from before), and its calls are refused.
    pub limit: Option<Option<f64>>,
    /// What the gateway charged it so far: the `charged_usd` of the calls it recorded itself.
    pub spent: f64,
    /// The gateway recorded the limit reached already.
    pub refused: bool,
    pub hold: Hold,
    /// How many agents its team has.
    pub agents: usize,
}

/// Clears a pause and extends the branch's live tokens by how long it lasted, in one statement.
const RESUME: &str = "WITH was AS (
    UPDATE branches b SET hold = NULL, paused_at = NULL
      FROM (SELECT paused_at FROM branches WHERE id = $1) AS prior
     WHERE b.id = $1 AND b.hold = 'paused'
    RETURNING greatest(extract(epoch FROM now() - prior.paused_at), 0)::float8 AS secs),
  extended AS (
    UPDATE run_tokens SET expires_at = expires_at + make_interval(secs => (SELECT secs FROM was))
     WHERE branch_id = $1 AND revoked_at IS NULL AND EXISTS (SELECT 1 FROM was))
  SELECT 1 FROM was";
/// Puts a branch on a pause or a stop (a pause remembers when it began).
const HOLD: &str = "UPDATE branches SET hold = $2,
      paused_at = CASE WHEN $2 = 'paused' THEN now() END
    WHERE id = $1 AND (hold IS NULL OR hold = 'paused') AND hold IS DISTINCT FROM $2
    RETURNING 1";

const CONTROLLED: &str = "SELECT b.rules IS NOT NULL, (b.rules->>'max_cost_usd')::float8, b.hold,
    jsonb_array_length(coalesce(b.team->'agents', '[]'::jsonb)),
    (SELECT coalesce(sum((payload->>'charged_usd')::float8), 0)::float8 FROM events
      WHERE branch_id = $1 AND kind = 'model_call' AND link_n IS NULL),
    EXISTS (SELECT 1 FROM events WHERE branch_id = $1 AND kind = 'world_event'
      AND link_n IS NULL AND payload->>'kind' = 'limit_reached')
  FROM branches b WHERE b.id = $1";

impl Scoped<'_> {
    /// Keeps `rules` as the ones the control box planned `branch_id` with.
    pub async fn set_rules(&self, branch_id: &str, rules: &Rules) -> Result<()> {
        let sql = "UPDATE branches SET rules = $2 WHERE id = $1";
        let rules = serde_json::to_value(rules)?;
        self.execute(sql, &[&branch_id, &rules]).await?;
        Ok(())
    }

    /// Gives the fork `child` the rules on record of its `parent` (a fork changes none).
    pub async fn inherit_rules(&self, child: &str, parent: &str) -> Result<()> {
        let sql = "UPDATE branches SET rules = (SELECT rules FROM branches WHERE id = $2)
                   WHERE id = $1";
        self.execute(sql, &[&child, &parent]).await?;
        Ok(())
    }

    /// Puts `branch_id` on `hold`: whether it changed. A stop is final (never turned into a
    /// pause or another stop); going on clears only a pause, and extends the branch's tokens by
    /// how long it lasted (a pause keeps when it began).
    pub async fn set_hold(&self, branch_id: &str, hold: Hold) -> Result<bool> {
        let row = match hold.stored() {
            None => self.query_opt(RESUME, &[&branch_id]).await?,
            Some(h) => self.query_opt(HOLD, &[&branch_id, &h]).await?,
        };
        Ok(row.is_some())
    }

    /// The hold on record of `branch_id` (running when there is none, or no such branch).
    pub async fn hold(&self, branch_id: &str) -> Result<Hold> {
        let sql = "SELECT hold FROM branches WHERE id = $1";
        let row = self.query_opt(sql, &[&branch_id]).await?;
        let stored = row.and_then(|r| r.get::<_, Option<String>>(0));
        Ok(Hold::from_stored(stored.as_deref()))
    }

    /// What the control box keeps of `branch_id`; None when there is no such branch.
    pub async fn controlled(&self, branch_id: &str) -> Result<Option<Controlled>> {
        let Some(r) = self.query_opt(CONTROLLED, &[&branch_id]).await? else {
            return Ok(None);
        };
        let limit = r.get::<_, bool>(0).then(|| r.get::<_, Option<f64>>(1));
        let hold = Hold::from_stored(r.get::<_, Option<String>>(2).as_deref());
        Ok(Some(Controlled {
            limit,
            hold,
            agents: usize::try_from(r.get::<_, i32>(3)).unwrap_or(0),
            spent: r.get(4),
            refused: r.get(5),
        }))
    }

    /// True when `branch_id` runs in a sandbox: it is running and a worker token was made for it
    /// (live or not: a long pause may outlast it until the resume extends it).
    pub async fn is_linked(&self, branch_id: &str) -> Result<bool> {
        let sql = "SELECT EXISTS (SELECT 1 FROM branches b JOIN run_tokens t ON t.branch_id = b.id
                   WHERE b.id = $1 AND b.status = 'running' AND t.kind = 'worker')";
        let row = self.query_one(sql, &[&branch_id]).await?;
        Ok(row.get(0))
    }
}
