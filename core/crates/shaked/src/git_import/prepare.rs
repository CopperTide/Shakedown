//! Fetching a repo (or unpacking an archive) and working out what importing it would do, without
//! installing or writing.

use std::path::{Path, PathBuf};

use parts::Hub;

use super::record::{self, imports_dir, Record};
use super::{
    archive, default_id, detect, fetch, git_url, items, python, ImportError, Request, Source,
};

/// A fetched repo and what importing it would do.
pub struct Prepared {
    pub id: String,
    /// `git` or `archive`.
    pub source: &'static str,
    /// The git URL, or the archive's file name.
    pub url: String,
    /// The git ref (empty for an archive).
    pub git_ref: String,
    /// The commit, or the archive's SHA-256.
    pub commit: String,
    /// A runtime install: the folder whose packages are installed (no project files).
    pub packages_only: Option<String>,
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
    let id = checked_id(
        req.id
            .clone()
            .unwrap_or_else(|| default_id(source_name(&req.source))),
    )?;
    let dir = imports_dir(project).join(&id);
    let last = record::read(&dir);
    let packages_only = scope(&req, last.as_ref())?;
    let (source, url, git_ref, fetched) = fetched(req.source, &dir).await?;
    let checkout = fetched.dir.canonicalize().unwrap_or(fetched.dir);
    let mut found = detect::detect(&checkout);
    if let Some(folder) = &packages_only {
        only_packages_under(&mut found, &checkout, folder);
    }
    let items = match &found.project {
        Some(repo) => items::plan(repo, project, last.as_ref()),
        None => vec![],
    };
    let (envs, sources) = python_plan(&dir, &fetched.commit, &checkout, &found);
    let conflicts = conflicts(&id, &found, hub);
    Ok(Prepared {
        id,
        source,
        url,
        git_ref,
        commit: fetched.commit,
        packages_only,
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

/// What names the import by default: the git URL or the archive's file name.
fn source_name(source: &Source) -> &str {
    match source {
        Source::Git { url, .. } => url,
        Source::Archive { name, .. } => archive::stem(name),
    }
}

/// The source fetched into the import folder `dir`: its kind, URL (or file name), ref and
/// checkout.
async fn fetched(
    source: Source,
    dir: &Path,
) -> Result<(&'static str, String, String, fetch::Fetched), ImportError> {
    match source {
        Source::Git { url, git_ref } => {
            let (git, r) = (git_url(&url)?, checked_ref(git_ref)?);
            let fetched = fetch::fetch(&git, &r, dir).await?;
            Ok(("git", url, r, fetched))
        }
        Source::Archive { name, bytes } => {
            let fetched = archive::unpack(&name, &bytes, dir)?;
            Ok(("archive", name, String::new(), fetched))
        }
    }
}

/// The folder a runtime install takes packages from, checked; refused into an import that wrote
/// project files (that one is updated with Import from git).
fn scope(req: &Request, last: Option<&Record>) -> Result<Option<String>, ImportError> {
    let Some(folder) = &req.packages_only else {
        return Ok(None);
    };
    if let Some(last) = last.filter(|l| !l.items.is_empty()) {
        return Err(ImportError::Conflict(format!(
            "{} was imported from git with its project files: update it with Import from git, or give this runtime another id",
            last.id
        )));
    }
    let folder = folder.trim().trim_matches('/');
    let plain = folder
        .split('/')
        .all(|s| !s.is_empty() && s != "." && s != "..");
    match folder.is_empty() || (plain && !folder.contains('\\')) {
        true => Ok(Some(folder.to_string())),
        false => Err(ImportError::Bad(format!(
            "{folder:?} is not a folder of the repo: give a relative path such as packages/desk"
        ))),
    }
}

/// Keeps only the packages under `folder` of the checkout, and no project or Compose files.
fn only_packages_under(found: &mut detect::Found, checkout: &Path, folder: &str) {
    found.project = None;
    found.compose.clear();
    if !folder.is_empty() {
        if !checkout.join(folder).is_dir() {
            found
                .problems
                .push(format!("the repo has no folder {folder}"));
        }
        let under = |rel: &str| rel == folder || rel.starts_with(&format!("{folder}/"));
        found.packages.retain(|p| under(&p.rel));
    }
    if found.packages.is_empty() {
        let at = if folder.is_empty() {
            "the repo"
        } else {
            folder
        };
        (found.problems).push(format!("{at} holds no package (no shakedown-package.toml)"));
    }
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
