//! Fetching a repo and working out what importing it would do, without installing or writing.

use std::path::{Path, PathBuf};

use parts::Hub;

use super::record::{self, imports_dir, Record};
use super::{default_id, detect, fetch, git_url, items, python, ImportError, Request};

/// A fetched repo and what importing it would do.
pub struct Prepared {
    pub id: String,
    pub url: String,
    pub git_ref: String,
    pub commit: String,
    /// `.shakedown/imports/<id>`.
    pub dir: PathBuf,
    pub checkout: PathBuf,
    pub found: detect::Found,
    pub items: Vec<items::ItemPlan>,
    pub envs: Vec<Env>,
    pub sources: python::Sources,
    pub conflicts: Vec<String>,
    /// The last import of this id.
    pub last: Option<Record>,
}

/// A uv workspace in the checkout and the environment it is installed into.
pub struct Env {
    pub root: PathBuf,
    pub dir: PathBuf,
}

pub async fn prepare(
    project: &Path,
    req: Request,
    hub: Option<&Hub>,
) -> Result<Prepared, ImportError> {
    let (url, id, git_ref) = checked(&req)?;
    let dir = imports_dir(project).join(&id);
    let fetched = fetch::fetch(&url, &git_ref, &dir).await?;
    let checkout = fetched.dir.canonicalize().unwrap_or(fetched.dir);
    let found = detect::detect(&checkout);
    let last = record::read(&dir);
    let items = match &found.project {
        Some(repo) => items::plan(repo, project, last.as_ref()),
        None => vec![],
    };
    let (envs, sources) = python_plan(&dir, &fetched.commit, &checkout, &found);
    let conflicts = conflicts(&id, &found, hub);
    Ok(Prepared {
        id,
        url: req.url,
        git_ref,
        commit: fetched.commit,
        dir,
        checkout,
        found,
        items,
        envs,
        sources,
        conflicts,
        last,
    })
}

/// The git URL, the import id and the ref `req` asks for, each checked.
fn checked(req: &Request) -> Result<(String, String, String), ImportError> {
    let url = git_url(&req.url)?;
    let id = checked_id(req.id.clone().unwrap_or_else(|| default_id(&req.url)))?;
    Ok((url, id, checked_ref(req.git_ref.clone())?))
}

/// An import id: an item-style id of at most 40 characters (it names a folder).
fn checked_id(id: String) -> Result<String, ImportError> {
    match shakedown_project::Id::parse(&id).is_ok() && id.len() <= 40 {
        true => Ok(id),
        false => Err(ImportError::Bad(format!(
            "{id:?} is not an import id: lowercase letters, digits and dashes, at most 40"
        ))),
    }
}

/// The ref to fetch (default `HEAD`); one that could be read as an option is refused.
fn checked_ref(git_ref: Option<String>) -> Result<String, ImportError> {
    let r = git_ref
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "HEAD".into());
    match r.starts_with('-') || r.contains(char::is_whitespace) {
        true => Err(ImportError::Bad(format!("{r:?} is not a ref"))),
        false => Ok(r),
    }
}

/// The uv workspaces the packages run in, their environments, and the edits their pyprojects
/// need.
fn python_plan(
    dir: &Path,
    commit: &str,
    checkout: &Path,
    found: &detect::Found,
) -> (Vec<Env>, python::Sources) {
    let mut roots: Vec<PathBuf> = (found.packages.iter())
        .filter_map(|p| p.uv_root.clone())
        .collect();
    roots.sort();
    roots.dedup();
    let kits = python::kits(&crate::serve::repo_dir());
    let mut all = python::Sources::default();
    for root in &roots {
        all.merge(python::sources(checkout, root, &kits));
    }
    let (short, env_root) = (fetch::short(commit), dir.join("env"));
    let envs = roots
        .into_iter()
        .map(|root| Env {
            dir: python::env_dir(&env_root, short, checkout, &root),
            root,
        })
        .collect();
    (envs, all)
}

/// Package names that clash: twice in the repo, or installed already by something else.
fn conflicts(id: &str, found: &detect::Found, hub: Option<&Hub>) -> Vec<String> {
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    for p in &found.packages {
        let name = &p.manifest.name;
        if !seen.insert(name.clone()) {
            out.push(format!("the repo has two packages named {name}"));
        } else if let Some(dir) = hub.and_then(|h| h.taken(id, name)) {
            out.push(format!(
                "a package named {name} is installed already, from {}",
                dir.display()
            ));
        }
    }
    out
}
