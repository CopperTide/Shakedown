//! A world built on the control box, copied into a branch's sandbox. The template brings the
//! installed packages; a world the builder made lives only in the control box's worlds folder.
//! When the branch's benchmark is such a world, its folder is written into the sandbox's worlds
//! folder ([`SANDBOX_WORLDS`]) before the ticket: the worker scans that folder once it starts.
//! Hidden entries, `__pycache__`, `node_modules` and symbolic links are left out.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use super::{ProviderRef, SandboxBackend};
use crate::app::AppState;

/// Where a sandbox's worker looks for built worlds (the template's `SHAKEDOWN_WORLDS`).
pub const SANDBOX_WORLDS: &str = "/var/lib/shakedown/worlds";
/// The most a world's files may weigh to be copied into a sandbox.
const MAX_WORLD_BYTES: u64 = 64 << 20;

/// A file to copy: where it goes in the sandbox, and where it is here.
pub(super) type WorldFile = (String, PathBuf);

/// The files of `benchmark` to copy into a sandbox: none unless it is a world built in `state`'s
/// worlds folder.
pub(super) fn world_files(state: &AppState, benchmark: &str) -> Result<Vec<WorldFile>> {
    let Some(worlds) = state.config.worlds_dir.as_deref() else {
        return Ok(vec![]);
    };
    let dir = state.hub.registry().get(benchmark).map(|m| m.dir.clone());
    let Some(dir) = dir.filter(|d| d.starts_with(worlds)) else {
        return Ok(vec![]);
    };
    let name = dir.file_name().unwrap_or_default().to_string_lossy();
    let mut files = vec![];
    walk(&dir, &format!("{SANDBOX_WORLDS}/{name}"), &mut files)?;
    let mut bytes = 0;
    for (_, local) in &files {
        bytes += std::fs::metadata(local)?.len();
    }
    if bytes > MAX_WORLD_BYTES {
        bail!("the world {benchmark} is {bytes} bytes, over {MAX_WORLD_BYTES} for a sandbox");
    }
    Ok(files)
}

fn left_out(name: &str) -> bool {
    name.starts_with('.') || name == "__pycache__" || name == "node_modules"
}

/// The files under `dir`, each with its path under `at`.
fn walk(dir: &Path, at: &str, out: &mut Vec<WorldFile>) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_string();
        let (kind, to) = (e.file_type()?, format!("{at}/{name}"));
        if left_out(&name) || kind.is_symlink() {
            continue;
        }
        match kind.is_dir() {
            true => walk(&e.path(), &to, out)?,
            false => out.push((to, e.path())),
        }
    }
    Ok(())
}

/// Writes `files` into `sb`.
pub(super) async fn copy(
    backend: &dyn SandboxBackend,
    sb: &ProviderRef,
    files: Vec<WorldFile>,
) -> Result<()> {
    for (to, local) in files {
        let bytes = tokio::fs::read(&local).await?;
        backend.write_file(sb, &to, bytes).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_folder_is_walked_without_hidden_or_cached_files() {
        let dir = std::env::temp_dir().join(format!("shaked-walk-{}", uuid::Uuid::new_v4()));
        for f in [
            "w/a.toml",
            "w/data/b.json",
            "w/.venv/x",
            "w/__pycache__/c.pyc",
        ] {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        let mut out = vec![];
        walk(&dir.join("w"), "/s/w", &mut out).unwrap();
        let mut at: Vec<&str> = out.iter().map(|(to, _)| to.as_str()).collect();
        at.sort();
        assert_eq!(at, ["/s/w/a.toml", "/s/w/data/b.json"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
