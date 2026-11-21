//! Probes: docs read over HTTP from the project's services. Each probe GETs a path of a compose
//! service's published port (2 s timeout) with its headers; `{world_ms}` and `{world_s}` in the
//! path are the world's time. The doc is the answer's JSON body, or `{error}` when the service
//! can't be reached, answers other than 2xx, or answers no JSON. A save keeps the docs its probes
//! gave in `probes.json`, which `inspect` of the save reads.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use shakedown_part::helpers::blocking;
use shakedown_part::helpers::compose::Project;
use shakedown_part::helpers::http::reqwest::Client;

const FILE: &str = "probes.json";
const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProbeSpec {
    /// The doc's key in `inspect`.
    pub key: String,
    pub service: String,
    pub port: u16,
    pub path: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

impl ProbeSpec {
    /// The path with the world's time (unix ms) filled in.
    pub fn path_at(&self, world_ms: i64) -> String {
        (self.path)
            .replace("{world_ms}", &world_ms.to_string())
            .replace("{world_s}", &world_ms.div_euclid(1000).to_string())
    }
}

/// The docs of `specs` whose key `wanted` accepts, at world time `world_ms`.
pub async fn read_all(
    project: &Project,
    specs: &[ProbeSpec],
    wanted: impl Fn(&str) -> bool,
    world_ms: i64,
) -> Map<String, Value> {
    let mut docs = Map::new();
    for s in specs.iter().filter(|s| wanted(&s.key)) {
        let doc = read(project, s, world_ms).await;
        docs.insert(s.key.clone(), doc.unwrap_or_else(|e| json!({"error": e})));
    }
    docs
}

/// One probe's doc, or why there is none.
pub async fn read(project: &Project, s: &ProbeSpec, world_ms: i64) -> Result<Value, String> {
    let (p, service, port) = (project.clone(), s.service.clone(), s.port);
    let at = blocking(move || p.port(&service, port)).await;
    let at = at.map_err(|e| format!("{e:#}"))?;
    let client = Client::builder().timeout(TIMEOUT).build();
    let mut get =
        (client.map_err(|e| e.to_string())?).get(format!("http://{at}{}", s.path_at(world_ms)));
    for (k, v) in &s.headers {
        get = get.header(k, v);
    }
    let answer = get.send().await;
    let answer = answer.map_err(|e| format!("{} {}: {e}", s.service, s.port))?;
    let status = answer.status();
    let text = answer.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{} answered {status}: {}", s.service, cut(&text)));
    }
    let not_json = |_| format!("{} answered no JSON: {}", s.service, cut(&text));
    serde_json::from_str(&text).map_err(not_json)
}

/// At most 200 characters of an answer, for an error.
fn cut(text: &str) -> String {
    text.chars().take(200).collect()
}

pub fn write(dir: &Path, docs: &Map<String, Value>) -> Result<()> {
    std::fs::write(dir.join(FILE), serde_json::to_vec(docs)?)?;
    Ok(())
}

/// The docs a save's probes gave (none for a save without probes).
pub fn read_saved(dir: &Path) -> Map<String, Value> {
    let text = std::fs::read(dir.join(FILE)).unwrap_or_default();
    serde_json::from_slice(&text).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_time_fills_the_path() {
        let s = ProbeSpec {
            key: "k".into(),
            service: "s".into(),
            port: 1,
            path: "/slo?time={world_s}&ms={world_ms}".into(),
            headers: BTreeMap::new(),
        };
        assert_eq!(
            s.path_at(1_773_122_405_999),
            "/slo?time=1773122405&ms=1773122405999"
        );
    }
}
