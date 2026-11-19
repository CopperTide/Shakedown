//! A Twenty CRM part's REST API as a benchmark's seed uses it (feature `http`): records written
//! through the part's `connect` info, waiting out Twenty's rate limit.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

pub struct Crm {
    client: reqwest::Client,
    base: String,
    key: String,
}

/// The most records Twenty's batch endpoint takes in one request.
const BATCH: usize = 60;

impl Crm {
    /// `info` is the crm part's `connect` info.
    pub fn connect(info: &Value) -> Result<Crm> {
        let text = |name: &str| {
            info[name]
                .as_str()
                .ok_or_else(|| anyhow!("the crm part's connect info has no {name}"))
        };
        Ok(Crm {
            client: reqwest::Client::new(),
            base: text("url")?.trim_end_matches('/').to_string(),
            key: text("api_key")?.to_string(),
        })
    }

    /// Posts `body` to `/rest/<path>`. Twenty limits an API key to 100 requests a minute, so a
    /// 429 waits as long as it says and is sent again.
    pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let at = format!("{}/rest/{path}", self.base);
        loop {
            let answer = self
                .client
                .post(&at)
                .bearer_auth(&self.key)
                .json(body)
                .send()
                .await?;
            let status = answer.status();
            let text = answer.text().await?;
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if status.as_u16() == 429 {
                let wait = v["retryAfterSeconds"].as_u64().unwrap_or(5).max(1);
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                continue;
            }
            if !status.is_success() {
                bail!("writing to {path}: {status}: {text}");
            }
            return Ok(v);
        }
    }

    /// Writes `body` to `/rest/<path>`; the new record's id, from `create<made>`.
    pub async fn create(&self, path: &str, made: &str, body: &Value) -> Result<String> {
        let v = self.post(path, body).await?;
        let id = v["data"][format!("create{made}")]["id"].as_str();
        id.map(str::to_string)
            .ok_or_else(|| anyhow!("no id in {v}"))
    }

    /// Writes `bodies` through `/rest/batch/<path>`, [`BATCH`] to a request.
    pub async fn create_all(&self, path: &str, bodies: &[Value]) -> Result<()> {
        for chunk in bodies.chunks(BATCH) {
            self.post(&format!("batch/{path}"), &json!(chunk)).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_needs_the_parts_url_and_key() {
        assert!(Crm::connect(&json!({"url": "u"})).is_err());
        let crm = Crm::connect(&json!({"url": "http://h/", "api_key": "k"})).unwrap();
        assert_eq!((crm.base.as_str(), crm.key.as_str()), ("http://h", "k"));
    }
}
