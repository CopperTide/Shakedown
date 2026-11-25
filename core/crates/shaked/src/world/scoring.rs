//! Scoring a world by the benchmark's named checks: every part's end state, read with `inspect`.

use parts::proto::{Docs, InspectParams, NamedCheck, Score, ScoreParams};
use serde_json::{Map, Value};

use super::{models::SCORING, request, World, WorldError};

impl World {
    /// `{part: {key: doc}}` of every part's live instance.
    async fn end_state(&self) -> Result<Map<String, Value>, WorldError> {
        let mut out = Map::new();
        for p in &self.parts {
            let params = InspectParams {
                target: p.target(),
                keys: vec![],
            };
            let docs: Docs = request(&p.client, &p.label(), &params).await?;
            out.insert(p.name.clone(), Value::Object(docs.docs));
        }
        Ok(out)
    }

    /// The benchmark's named checks of this world's end state for `task`; the scoring may call
    /// models (a judge) on the world's endpoint, as the actor `world.scoring`.
    pub async fn score(&self, task: &str) -> Result<Vec<NamedCheck>, WorldError> {
        let params = ScoreParams {
            task: task.to_string(),
            docs: self.end_state().await?,
            models: self.models.as_ref().map(|m| m.of(SCORING)),
        };
        let score: Score = request(self.bench_client()?, &self.bench_label(), &params).await?;
        Ok(score.checks)
    }
}
