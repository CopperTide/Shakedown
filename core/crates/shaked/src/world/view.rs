//! The world's view (its digital twin), drawn by the benchmark from the world's connections and
//! the `inspect` docs of the parts `describe.view.docs` names.

use parts::proto::{Docs, InspectParams, ViewParams};
use serde_json::{Map, Value};

use super::{request, World, WorldError};

impl World {
    /// True when the benchmark draws the world's view.
    pub fn draws_view(&self) -> bool {
        self.view.is_some()
    }

    /// `{part: {key: doc}}` of the live instances of the parts the view reads.
    async fn view_docs(&self) -> Result<Map<String, Value>, WorldError> {
        let names = self.view.iter().flat_map(|v| &v.docs);
        let mut out = Map::new();
        for name in names {
            let p = self.part(name)?;
            let params = InspectParams {
                target: p.target(),
                keys: vec![],
            };
            let docs: Docs = request(&p.client, &p.label(), &params).await?;
            out.insert(p.name.clone(), Value::Object(docs.docs));
        }
        Ok(out)
    }

    /// The benchmark's view of the world now (the `view` of its answer, as it drew it).
    pub async fn view(&self) -> Result<Value, WorldError> {
        let params = ViewParams {
            connections: self.connections.clone(),
            docs: self.view_docs().await?,
        };
        let answer: Value = request(self.bench_client()?, &self.bench_label(), &params).await?;
        Ok(answer["view"].clone())
    }
}
