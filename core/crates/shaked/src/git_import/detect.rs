//! What a checkout holds, found without running anything: its project folder, its package
//! manifests, its Compose files and the uv projects its Python packages run in.

use std::path::{Path, PathBuf};

use parts::{Manifest, Run};
use shakedown_project::Project;

/// What a checkout holds.
pub struct Found {
    /// The repo's own project folder (`shakedown.yaml` at its root).
    pub project: Option<Project>,
    pub packages: Vec<FoundPackage>,
    pub compose: Vec<ComposeFile>,
    pub problems: Vec<String>,
}

/// One package manifest in the checkout.
pub struct FoundPackage {
    pub manifest: Manifest,
    /// Its folder, relative to the checkout.
    pub rel: String,
    /// The root of the uv project (or workspace) its command runs in, when it runs `uv`.
    pub uv_root: Option<PathBuf>,
}

/// A Compose file outside every package: a candidate for the `service` integration.
pub struct ComposeFile {
    /// Relative to the checkout.
    pub rel: String,
    pub services: Vec<String>,
    pub volumes: Vec<String>,
}

/// Folders never searched: dot folders (`.git`, `.venv`), and build output.
fn skipped(name: &str) -> bool {
    name.starts_with('.') || ["node_modules", "target", "__pycache__"].contains(&name)
}

const COMPOSE: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// Searches `root`.
pub fn detect(root: &Path) -> Found {
    let mut problems = Vec::new();
    let project = repo_project(root, &mut problems);
    let (mut manifests, mut composes) = (Vec::new(), Vec::new());
    walk(root, &mut |dir| {
        if parts::manifest_file(dir).is_some() {
            manifests.push(dir.to_path_buf());
        }
        composes.extend(COMPOSE.iter().map(|n| dir.join(n)).filter(|f| f.is_file()));
    });
    let packages: Vec<FoundPackage> = manifests
        .iter()
        .filter_map(|dir| package(root, dir, &mut problems))
        .collect();
    let in_package = |f: &Path| packages.iter().any(|p| f.starts_with(&p.manifest.dir));
    let compose = composes
        .into_iter()
        .filter(|f| !in_package(f))
        .map(|f| compose_file(root, &f, &mut problems))
        .collect();
    Found {
        project,
        packages,
        compose,
        problems,
    }
}

/// The repo's own project folder, when `shakedown.yaml` is at its root.
fn repo_project(root: &Path, problems: &mut Vec<String>) -> Option<Project> {
    if !root.join("shakedown.yaml").is_file() {
        return None;
    }
    Project::open(root)
        .map_err(|e| problems.push(format!("shakedown.yaml: {e}")))
        .ok()
}

/// The package in `dir`, or the problem reading its manifest.
fn package(root: &Path, dir: &Path, problems: &mut Vec<String>) -> Option<FoundPackage> {
    match Manifest::load(dir) {
        Ok(manifest) => Some(FoundPackage {
            uv_root: uv_root(root, &manifest, problems),
            rel: rel(root, dir),
            manifest,
        }),
        Err(e) => {
            problems.push(format!("{}: {e:#}", rel(root, dir)));
            None
        }
    }
}

/// Calls `visit` on `dir` and every folder under it that is searched, in a stable order.
fn walk(dir: &Path, visit: &mut impl FnMut(&Path)) {
    visit(dir);
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| !skipped(&e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for d in dirs {
        walk(&d, visit);
    }
}

/// `path` relative to `root`, with `/` separators (`.` for the root itself).
pub fn rel(root: &Path, path: &Path) -> String {
    let r = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
    match r.as_ref() {
        "" => ".".into(),
        r => r.to_string(),
    }
}

/// The uv project root a `uv` command runs in: its `--project` (or `--directory`) folder, else
/// the package's, then up to the nearest uv workspace that holds it, never above `root`.
fn uv_root(root: &Path, m: &Manifest, problems: &mut Vec<String>) -> Option<PathBuf> {
    let Run::Command(argv) = &m.run else {
        return None;
    };
    if argv.first().map(|a| a.rsplit('/').next() == Some("uv")) != Some(true) {
        return None;
    }
    let base = flag_value(argv, &["--project", "--directory"])
        .map(|p| m.dir.join(p))
        .unwrap_or_else(|| m.dir.clone());
    let base = base.canonicalize().unwrap_or(base);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let inside: Vec<&Path> = base
        .ancestors()
        .take_while(|a| a.starts_with(&root))
        .collect();
    let has = |d: &&&Path| d.join("pyproject.toml").is_file();
    let Some(project) = inside.iter().find(has) else {
        problems.push(format!(
            "{} runs uv, but no pyproject.toml holds {}",
            m.name,
            rel(&root, &base)
        ));
        return None;
    };
    let workspace = inside
        .iter()
        .filter(|d| d.starts_with(project) || project.starts_with(d))
        .find(|d| is_workspace(&d.join("pyproject.toml")));
    Some(workspace.unwrap_or(project).to_path_buf())
}

/// The value after the first of `flags` in `argv` (`--flag value` or `--flag=value`).
fn flag_value<'a>(argv: &'a [String], flags: &[&str]) -> Option<&'a str> {
    for (i, a) in argv.iter().enumerate() {
        for f in flags {
            if a == f {
                return argv.get(i + 1).map(String::as_str);
            }
            if let Some(v) = a.strip_prefix(&format!("{f}=")) {
                return Some(v);
            }
        }
    }
    None
}

/// True when the pyproject at `file` declares a uv workspace.
fn is_workspace(file: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(file) else {
        return false;
    };
    let Ok(doc) = text.parse::<toml::Table>() else {
        return false;
    };
    let uv = doc.get("tool").and_then(|t| t.get("uv"));
    uv.and_then(|u| u.get("workspace")).is_some()
}

fn compose_file(root: &Path, file: &Path, problems: &mut Vec<String>) -> ComposeFile {
    let rel = rel(root, file);
    let doc: serde_yaml_ng::Value = match std::fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_yaml_ng::from_str(&t).map_err(|e| e.to_string()))
    {
        Ok(doc) => doc,
        Err(e) => {
            problems.push(format!("{rel}: {e}"));
            serde_yaml_ng::Value::Null
        }
    };
    let keys = |section: &str| -> Vec<String> {
        doc.get(section)
            .and_then(serde_yaml_ng::Value::as_mapping)
            .map(|m| {
                m.keys()
                    .filter_map(|k| k.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    ComposeFile {
        services: keys("services"),
        volumes: keys("volumes"),
        rel,
    }
}
