//! `worlds/<id>/world.yaml`: the production systems before any agent.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Format, Invalid};

/// A world: connectors, the software flows between them, the data runs start from, and people.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldFile {
    /// Always `shakedown-world/3`.
    pub format: String,
    /// The world's display name.
    pub label: String,
    /// The installed package (a benchmark or a built world) whose runtime runs this world until
    /// worlds run from their files alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// The systems agents work against.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connectors: Vec<Connector>,
    /// Deterministic automations that already run in production.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flows: Vec<Flow>,
    /// What every run starts from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<DataSource>,
    /// Simulated people agents can ask.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<Person>,
}

impl Format for WorldFile {
    const FORMAT: &'static str = "shakedown-world/3";

    fn check(&self) -> Result<(), Invalid> {
        for (i, source) in self.data.iter().enumerate() {
            source
                .check()
                .map_err(|m| Invalid::new(format!("/data/{i}"), m))?;
        }
        Ok(())
    }
}

/// A system agents can call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    /// Unique within the world; tools are named `<id>.<tool>`.
    pub id: String,
    /// Display name.
    pub label: String,
    /// How runs use the system.
    pub mode: ConnectorMode,
    /// A built-in product connector, e.g. `sap-s4hana`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// An MCP server URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<String>,
    /// An OpenAPI spec file in the world's data folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openapi: Option<String>,
    /// Where the credential comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
}

/// How runs use a connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectorMode {
    /// A built-in stand-in seeded from the world's data.
    StandIn,
    /// The real system, through a sandboxed copy.
    Real,
    /// Recorded once, then replayed.
    Replay,
}

/// A credential reference: a key-store name or an environment variable, never the value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Auth {
    /// A key-store name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// An environment variable name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
}

/// A deterministic software flow: a trigger and the steps it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    /// Unique within the world.
    pub id: String,
    /// The event or schedule that starts it.
    pub trigger: String,
    /// Connector tools it calls, in order.
    pub steps: Vec<String>,
}

/// One source of the data runs start from: a file, a database or a connector import.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataSource {
    /// A file in the world's data folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// A database restored at the start of every run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// A read-only snapshot taken through a connector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import: Option<String>,
    /// The connector a file seeds, or an import reads from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seeds: Option<String>,
    /// The connector an import comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// A database engine, e.g. `postgres`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// The exact server version, e.g. `15.6`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The snapshot runs restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    /// Personal columns are masked before the snapshot is stored.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub masked: bool,
    /// Every scenario starts from this source.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default_state: bool,
}

impl DataSource {
    fn check(&self) -> Result<(), String> {
        let kinds = [&self.file, &self.database, &self.import]
            .iter()
            .filter(|k| k.is_some())
            .count();
        if kinds == 1 {
            Ok(())
        } else {
            Err("a data source is exactly one of file, database or import".to_string())
        }
    }
}

/// A simulated person agents can ask.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Person {
    /// Unique within the world.
    pub id: String,
    /// Display name.
    pub label: String,
    /// What they do, e.g. `planner`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}
