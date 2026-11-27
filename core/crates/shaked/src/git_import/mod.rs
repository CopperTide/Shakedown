//! Import from git: a repo's packages are installed and registered, and its worlds, scenarios,
//! setups and experiments copied into the project, so a world runs the systems the repo already
//! has (`docs/superpowers/specs/2026-11-22-git-import-design.md`).
//!
//! Layout, under the importing project: `.shakedown/imports/<id>/import.json` (the record),
//! `<commit12>/` (the checkout) and `env/<commit12>-<root>/` (a uv environment per workspace).
//!
//! The same machinery installs a runtime (`crate::runtimes`): packages only, from a git repo or
//! an uploaded archive (whose checkout is named after its SHA-256), with no project files.

mod apply;
mod archive;
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

pub use fetch::{default_id, git_url, short};
pub use register::{listed, register};
pub use report::package_kind;

/// Why an import stopped.
#[derive(Debug)]
pub enum ImportError {
    /// The request was malformed: a URL, a ref or an id.
    Bad(String),
    /// A package name is taken.
    Conflict(String),
    /// git or uv failed.
    Failed(String),
    /// No such import.
    NotFound(String),
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
            | ImportError::NotFound(m)
            | ImportError::Io(m) => m,
        }
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// Where an import comes from.
pub enum Source {
    Git {
        /// A git URL or an absolute path.
        url: String,
        /// A branch, a tag or a full commit SHA (default `HEAD`).
        git_ref: Option<String>,
    },
    /// An uploaded `.zip` or `.tar.gz` of a package folder or of a repo holding packages.
    Archive { name: String, bytes: Vec<u8> },
}

/// What to import.
pub struct Request {
    pub source: Source,
    /// The import's id (default: the repo's or the archive's name).
    pub id: Option<String>,
    /// A runtime install: only the packages under this folder of the repo (`""` for all of
    /// them), and none of its project files.
    pub packages_only: Option<String>,
}

impl Request {
    /// A whole repo from git, as Import from git asks for it.
    pub fn git(url: String, git_ref: Option<String>, id: Option<String>) -> Request {
        Request {
            source: Source::Git { url, git_ref },
            id,
            packages_only: None,
        }
    }
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
