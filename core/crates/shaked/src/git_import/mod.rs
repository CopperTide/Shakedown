//! Import from git: a repo's packages are installed and registered, and its worlds, scenarios,
//! setups and experiments copied into the project, so a world runs the systems the repo already
//! has (`docs/superpowers/specs/2026-11-22-git-import-design.md`).
//!
//! Layout, under the importing project: `.shakedown/imports/<id>/import.json` (the record),
//! `<commit12>/` (the checkout) and `env/<commit12>-<root>/` (a uv environment per workspace).

mod apply;
mod detect;
mod fetch;
mod items;
mod prepare;
mod python;
pub mod record;
mod register;
mod report;

use std::path::Path;

use parts::Hub;
use serde_json::{json, Value};

pub use fetch::{default_id, git_url};
pub use register::{listed, register};

/// Why an import stopped.
#[derive(Debug)]
pub enum ImportError {
    /// The request was malformed: a URL, a ref or an id.
    Bad(String),
    /// A package name is taken.
    Conflict(String),
    /// git or uv failed.
    Failed(String),
    /// A file could not be read or written.
    Io(String),
}

impl ImportError {
    fn io(action: &str, path: &Path, e: std::io::Error) -> ImportError {
        ImportError::Io(format!("{action} {}: {e}", path.display()))
    }

    pub fn message(&self) -> &str {
        match self {
            ImportError::Bad(m)
            | ImportError::Conflict(m)
            | ImportError::Failed(m)
            | ImportError::Io(m) => m,
        }
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// What to import.
pub struct Request {
    /// A git URL or an absolute path.
    pub url: String,
    /// A branch, a tag or a full commit SHA (default `HEAD`).
    pub git_ref: Option<String>,
    /// The import's id (default: the repo's name).
    pub id: Option<String>,
}

/// Fetches `req` into the project at `project` and answers the plan; with `confirm`, installs,
/// registers (with `hub`, when shaked runs) and writes it. One import at a time.
pub async fn import(
    project: &Path,
    req: Request,
    confirm: bool,
    hub: Option<&Hub>,
) -> Result<Value, ImportError> {
    static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one = ONE.lock().await;
    let prepared = prepare::prepare(project, req, hub).await?;
    if !confirm {
        let mut plan = report::plan_json(&prepared);
        plan["proposed"] = json!(true);
        return Ok(plan);
    }
    apply::apply(project, prepared, hub).await
}
