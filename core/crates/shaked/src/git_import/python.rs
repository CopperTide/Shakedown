//! Python packages: one uv environment per uv workspace, kept out of the checkout, with
//! Shakedown's own packages (the part kit, the SDK) taken from the Shakedown that imports.

use std::path::{Path, PathBuf};

use super::detect::rel;
use super::ImportError;

/// Shakedown's Python packages, by distribution name, and their folders in this checkout of
/// Shakedown.
pub fn kits(repo: &Path) -> Vec<(&'static str, PathBuf)> {
    let all = [
        ("shakedown-part", "kits/python"),
        ("arena-part", "kits/python/compat/arena-part"),
        ("shakedown-sdk", "sdk/python"),
    ];
    all.into_iter()
        .map(|(name, dir)| (name, repo.join(dir)))
        .filter(|(_, dir)| dir.join("pyproject.toml").is_file())
        .collect()
}

/// One edit to a pyproject in the checkout: a source of Shakedown's own package pointed at this
/// Shakedown's.
#[derive(Debug, Clone, PartialEq)]
pub struct KitEdit {
    /// The pyproject, relative to the checkout.
    pub file: String,
    pub package: String,
    /// The source it had.
    pub from: String,
    /// The folder it has now.
    pub to: String,
}

/// What the pyprojects under `root` (a uv workspace in `checkout`) need: the kit sources to
/// point at this Shakedown's, notes on other absolute paths that exist here, and problems for
/// those that don't.
#[derive(Default)]
pub struct Sources {
    pub edits: Vec<KitEdit>,
    /// The pyprojects to rewrite, with their new text.
    pub writes: Vec<(PathBuf, String)>,
    pub notes: Vec<String>,
    pub problems: Vec<String>,
}

impl Sources {
    /// Adds `other`'s edits, notes and problems to these.
    pub fn merge(&mut self, other: Sources) {
        self.edits.extend(other.edits);
        self.writes.extend(other.writes);
        self.notes.extend(other.notes);
        self.problems.extend(other.problems);
    }
}

/// Reads the pyprojects under `root` and works out their edits (nothing is written).
pub fn sources(checkout: &Path, root: &Path, kits: &[(&str, PathBuf)]) -> Sources {
    let mut out = Sources::default();
    for file in pyprojects(root) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let at = rel(checkout, &file);
        match text.parse::<toml::Table>() {
            Ok(doc) => {
                if let Some(new) =
                    uv_sources(&doc).and_then(|t| edit(&at, &text, t, kits, &mut out))
                {
                    out.writes.push((file, new));
                }
            }
            Err(_) => out.problems.push(format!("{at}: not valid TOML")),
        }
    }
    out
}

/// The pyproject `text` (at `at`, with the uv sources `table`) with its kit sources pointed at
/// this Shakedown's; None when it names none.
fn edit(
    at: &str,
    text: &str,
    table: &toml::Table,
    kits: &[(&str, PathBuf)],
    out: &mut Sources,
) -> Option<String> {
    let mut new = text.to_string();
    let before = out.edits.len();
    for (name, source) in table {
        let Some((_, kit)) = kits.iter().find(|(k, _)| k == name) else {
            check_path(at, name, source, out);
            continue;
        };
        match point_at(&new, name, kit) {
            Some(edited) => {
                new = edited;
                let (file, package) = (at.to_string(), name.clone());
                let (from, to) = (source.to_string(), kit.display().to_string());
                out.edits.push(KitEdit { file, package, from, to });
            }
            None => out.problems.push(format!(
                "{at}: the source of {name} can't be pointed at this Shakedown's (write it as `{name} = {{ path = ... }}` under [tool.uv.sources])"
            )),
        }
    }
    (out.edits.len() > before).then_some(new)
}

/// A path source outside the checkout: noted when it is on this machine, a problem when not.
fn check_path(at: &str, name: &str, source: &toml::Value, out: &mut Sources) {
    let Some(path) = source.get("path").and_then(toml::Value::as_str) else {
        return;
    };
    if !Path::new(path).is_absolute() {
        return;
    }
    match Path::new(path).exists() {
        true => out.notes.push(format!(
            "{at}: {name} comes from {path} on this machine; the repo needs it there"
        )),
        false => out.problems.push(format!(
            "{at}: {name} comes from {path}, which is not on this machine"
        )),
    }
}

/// `[tool.uv.sources]` of a pyproject.
fn uv_sources(doc: &toml::Table) -> Option<&toml::Table> {
    let uv = doc.get("tool")?.get("uv")?;
    uv.get("sources")?.as_table()
}

/// `text` with the line `name = ...` under `[tool.uv.sources]` replaced by a path to `kit`
/// (None when there is no such line).
fn point_at(text: &str, name: &str, kit: &Path) -> Option<String> {
    let mut section = String::new();
    let mut found = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let t = line.trim();
            if t.starts_with('[') {
                section = t.to_string();
            }
            let key = t.split('=').next().unwrap_or("").trim().trim_matches('"');
            if section == "[tool.uv.sources]" && t.contains('=') && key == name {
                found = true;
                let path = kit.display().to_string();
                return format!("{name} = {{ path = {path:?}, editable = true }}");
            }
            line.to_string()
        })
        .collect();
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    found.then_some(out)
}

/// Every pyproject.toml under `root` (dot folders and build output skipped).
fn pyprojects(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let file = dir.join("pyproject.toml");
        if file.is_file() {
            out.push(file);
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().to_string();
            let skip = name.starts_with('.')
                || ["node_modules", "target", "__pycache__"].contains(&name.as_str());
            if e.file_type().is_ok_and(|t| t.is_dir()) && !skip {
                stack.push(e.path());
            }
        }
    }
    out.sort();
    out
}

/// The environment of the uv workspace `root` at a commit: `<env_dir>/<commit12>-<root key>`.
pub fn env_dir(env_root: &Path, commit12: &str, checkout: &Path, root: &Path) -> PathBuf {
    let key: String = rel(checkout, root)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let key = if key == "-" { "root".to_string() } else { key };
    env_root.join(format!("{commit12}-{key}"))
}

/// `uv sync --all-packages` of the workspace `root` into `env` (its `UV_PROJECT_ENVIRONMENT`).
pub async fn install(root: &Path, env: &Path) -> Result<(), ImportError> {
    let out = tokio::process::Command::new("uv")
        .args(["sync", "--all-packages", "--quiet", "--project"])
        .arg(root)
        .env("UV_PROJECT_ENVIRONMENT", env)
        .env_remove("VIRTUAL_ENV")
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| ImportError::Failed(format!("running uv (is it installed?): {e}")))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = err.lines().rev().take(15).collect();
    let tail: Vec<&str> = tail.into_iter().rev().collect();
    Err(ImportError::Failed(format!(
        "uv sync in {}: {}",
        root.display(),
        tail.join("\n")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PYPROJECT: &str = r#"[project]
name = "driver"
dependencies = ["shakedown-part", "legality"]

[tool.uv.sources]
legality = { workspace = true }
shakedown-part = { path = "/home/someone/Shakedown/kits/python", editable = true }
elsewhere = { path = "/nowhere/at/all" }
"#;

    #[test]
    fn kit_sources_point_at_this_shakedown_and_missing_paths_are_problems() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("parts/driver")).unwrap();
        let file = dir.path().join("parts/driver/pyproject.toml");
        std::fs::write(&file, PYPROJECT).unwrap();
        let kit = dir.path().join("kit");
        let found = sources(dir.path(), dir.path(), &[("shakedown-part", kit.clone())]);
        assert_eq!(found.edits.len(), 1);
        let (edit, (at, text)) = (&found.edits[0], &found.writes[0]);
        assert_eq!(at, &file);
        assert_eq!(edit.file, "parts/driver/pyproject.toml");
        assert_eq!(edit.package, "shakedown-part");
        let want = format!(
            "shakedown-part = {{ path = {:?}, editable = true }}\n",
            kit.display().to_string()
        );
        assert!(text.contains(&want), "{text}");
        assert!(text.contains("legality = { workspace = true }\n"));
        assert_eq!(found.problems.len(), 1, "{:?}", found.problems);
        assert!(found.problems[0].contains("/nowhere/at/all"));
    }

    #[test]
    fn a_table_form_source_is_a_problem_not_a_silent_skip() {
        let text = "[tool.uv.sources.shakedown-part]\npath = \"/x\"\n";
        assert_eq!(point_at(text, "shakedown-part", Path::new("/k")), None);
    }
}
