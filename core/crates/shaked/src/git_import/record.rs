//! `<project>/.shakedown/imports/<id>/import.json`: what an import fetched, registered and wrote.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use shakedown_project::format::Kind;
use shakedown_project::kind_name;

use super::ImportError;

/// One import of a repo into a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    /// `git` or `archive`.
    #[serde(default = "git")]
    pub source: String,
    /// A runtime install: the folder of the repo whose packages it installed (no project files).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packages_only: Option<String>,
    /// The git URL, or the archive's file name.
    pub url: String,
    /// The git ref (empty for an archive).
    #[serde(rename = "ref")]
    pub git_ref: String,
    /// The commit, or the archive's SHA-256.
    pub commit: String,
    /// RFC 3339.
    pub imported_at: String,
    pub packages: Vec<RecordedPackage>,
    pub items: Vec<RecordedItem>,
    /// The edits shaked made to the checkout (kit sources), as `<file>: <package> -> <folder>`.
    #[serde(default)]
    pub edits: Vec<String>,
}

fn git() -> String {
    "git".into()
}

/// A package the import registered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedPackage {
    pub name: String,
    pub kind: String,
    /// Its folder, relative to the checkout.
    pub dir: String,
    /// Its uv environment, relative to the import's folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
}

/// An item the project holds from this import, with the digest it was written with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedItem {
    pub kind: String,
    pub id: String,
    pub digest: String,
}

impl Record {
    /// The item of `kind` named `id`, if this import wrote it.
    pub fn item(&self, kind: Kind, id: &str) -> Option<&RecordedItem> {
        let kind = kind_name(kind);
        self.items.iter().find(|i| i.kind == kind && i.id == id)
    }

    /// The checkout folder under the import's folder `dir`.
    pub fn checkout(&self, dir: &Path) -> PathBuf {
        dir.join(super::fetch::short(&self.commit))
    }
}

/// `<project>/.shakedown/imports`.
pub fn imports_dir(project: &Path) -> PathBuf {
    project.join(".shakedown").join("imports")
}

const FILE: &str = "import.json";

/// The record in the import folder `dir`, if there is one.
pub fn read(dir: &Path) -> Option<Record> {
    let text = std::fs::read_to_string(dir.join(FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes `record` into the import folder `dir`.
pub fn write(dir: &Path, record: &Record) -> Result<(), ImportError> {
    let text =
        serde_json::to_string_pretty(record).map_err(|e| ImportError::Failed(e.to_string()))?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, text).map_err(|e| ImportError::io("writing", &tmp, e))?;
    std::fs::rename(&tmp, dir.join(FILE)).map_err(|e| ImportError::io("writing", &tmp, e))
}

/// Every import recorded in the project at `project`, by id.
pub fn all(project: &Path) -> Vec<(PathBuf, Record)> {
    let Ok(rd) = std::fs::read_dir(imports_dir(project)) else {
        return vec![];
    };
    let mut out: Vec<(PathBuf, Record)> = rd
        .filter_map(Result::ok)
        .filter_map(|e| Some((e.path(), read(&e.path())?)))
        .collect();
    out.sort_by(|a, b| a.1.id.cmp(&b.1.id));
    out
}
