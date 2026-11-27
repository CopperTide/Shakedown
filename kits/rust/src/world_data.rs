//! A world's own data (docs/protocol.md, "World data"): the files a project world marks "Every
//! run starts from this", which shaked gives every part in its `start` config, and the benchmark
//! in `gold_actions` and `score`, under `world_data`. Absent when the world has none: a part then
//! starts from what it carries itself.
//!
//! ```no_run
//! # use shakedown_part::world_data::WorldData;
//! # let config = serde_json::json!({});
//! if let Some(data) = WorldData::of(&config) {
//!     let db = data.read_json("db.json").expect("the world's db.json");
//! }
//! ```

use std::path::PathBuf;

use serde_json::Value;

/// The key that holds the data in a part's config and in `gold_actions` and `score` params.
pub const KEY: &str = "world_data";

/// One file of the data, at its path in the world's folder (e.g. `data/db.json`).
#[derive(Debug, Clone, PartialEq)]
pub struct DataFile {
    pub path: String,
    /// The connector of the world it seeds, when the world says.
    pub seeds: Option<String>,
    pub size: u64,
    pub sha256: String,
}

/// The data a part or benchmark call was given.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldData {
    /// Changes whenever any file does; it is part of the part's fingerprint.
    pub digest: String,
    /// The folder the files are in, at their paths: a snapshot that never changes.
    pub dir: PathBuf,
    /// The project world's id.
    pub world: String,
    pub files: Vec<DataFile>,
}

impl WorldData {
    /// The data in `value` (a part's config, or a call's params), if it has any.
    pub fn of(value: &Value) -> Option<WorldData> {
        let d = value.get(KEY)?;
        let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let files = (d.get("files")?.as_array()?.iter())
            .filter_map(|f| {
                Some(DataFile {
                    path: text(f, "path")?,
                    seeds: text(f, "seeds"),
                    size: f.get("size").and_then(Value::as_u64).unwrap_or(0),
                    sha256: text(f, "sha256").unwrap_or_default(),
                })
            })
            .collect();
        Some(WorldData {
            digest: text(d, "digest")?,
            dir: PathBuf::from(text(d, "dir")?),
            world: text(d, "world").unwrap_or_default(),
            files,
        })
    }

    /// The file named `name`: by its path, else the one file whose last segment it is.
    pub fn find(&self, name: &str) -> Option<&DataFile> {
        if let Some(f) = self.files.iter().find(|f| f.path == name) {
            return Some(f);
        }
        let last = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
        let mut by_name = self.files.iter().filter(|f| last(&f.path) == name);
        match (by_name.next(), by_name.next()) {
            (Some(f), None) => Some(f),
            _ => None,
        }
    }

    /// Where the file named `name` ([`find`](Self::find)) is on disk.
    pub fn path(&self, name: &str) -> Option<PathBuf> {
        self.find(name).map(|f| self.dir.join(&f.path))
    }

    /// The file named `name` as text.
    pub fn read_text(&self, name: &str) -> std::io::Result<String> {
        let missing = || {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("the world's data has no {name}"),
            )
        };
        std::fs::read_to_string(self.path(name).ok_or_else(missing)?)
    }

    /// The file named `name` as JSON.
    pub fn read_json(&self, name: &str) -> std::io::Result<Value> {
        let text = self.read_text(name)?;
        serde_json::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_data_is_read_from_a_config_and_files_are_found_by_path_or_name() {
        let dir = std::env::temp_dir().join(format!("kit-world-data-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join("data/db.json"), "{\"users\": 2}").unwrap();
        let config = json!({"domain": "x", "world_data": {"digest": "abc", "dir": dir, "world": "w",
            "files": [{"path": "data/db.json", "seeds": "desk", "size": 12, "sha256": "f"}]}});
        let d = WorldData::of(&config).unwrap();
        assert_eq!(
            (d.digest.as_str(), d.files[0].seeds.as_deref()),
            ("abc", Some("desk"))
        );
        assert_eq!(d.read_json("db.json").unwrap(), json!({"users": 2}));
        assert_eq!(d.read_json("data/db.json").unwrap(), json!({"users": 2}));
        assert!(d.read_text("policy.md").is_err());
        assert_eq!(WorldData::of(&json!({"domain": "x"})), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
