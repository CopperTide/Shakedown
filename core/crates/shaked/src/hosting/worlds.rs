//! A world built on the control box, copied into a branch's sandbox. The template brings the
//! installed packages; a world the builder made lives only in the control box's worlds folder.
//! When the branch's benchmark is such a world, its folder is written into the sandbox's worlds
//! folder ([`SANDBOX_WORLDS`]) before the ticket, one write per file: the worker scans that folder
//! once it starts. Only the files `world_files.toml` allows are copied (an allowlist: what the
//! world builder writes, never a name that holds a secret); symbolic links never are.

use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::Result;
use serde::Deserialize;

use super::{ProviderRef, SandboxBackend};
use crate::app::AppState;

/// Where a sandbox's worker looks for built worlds (the template's `SHAKEDOWN_WORLDS`).
pub const SANDBOX_WORLDS: &str = "/var/lib/shakedown/worlds";
/// The most a world's files may weigh to be copied into a sandbox, in MiB.
const MAX_WORLD_MIB: u64 = 64;

/// What a world may carry into a sandbox (`world_files.toml`).
#[derive(Deserialize)]
struct Allowed {
    extensions: Vec<String>,
    names: Vec<String>,
    never: Vec<String>,
    skip_dirs: Vec<String>,
}

static ALLOWED: LazyLock<Allowed> = LazyLock::new(|| {
    toml::from_str(include_str!("world_files.toml")).expect("world_files.toml is valid")
});

/// Why a built world was not copied into its sandbox: the branch's error, shown to the person.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct WorldNotCopied(pub String);

/// A file to copy: where it goes in the sandbox, and where it is here.
pub(super) type WorldFile = (String, PathBuf);

/// The files of `benchmark` to copy into a sandbox: none unless it is a world built in `state`'s
/// worlds folder. A world that can't be read, or weighs over [`MAX_WORLD_MIB`], is
/// [`WorldNotCopied`].
pub(super) fn world_files(state: &AppState, benchmark: &str) -> Result<Vec<WorldFile>> {
    let Some(worlds) = state.config.worlds_dir.as_deref() else {
        return Ok(vec![]);
    };
    let dir = state.hub.registry().get(benchmark).map(|m| m.dir.clone());
    let Some(dir) = dir.filter(|d| d.starts_with(worlds)) else {
        return Ok(vec![]);
    };
    let name = dir.file_name().unwrap_or_default().to_string_lossy();
    let files = listed(&dir, &format!("{SANDBOX_WORLDS}/{name}")).map_err(|e| {
        eprintln!("hosting: reading the world {benchmark}: {e:#}");
        WorldNotCopied(format!("the world {benchmark} could not be read"))
    })?;
    let mib = files
        .iter()
        .map(|(_, _, len)| len)
        .sum::<u64>()
        .div_ceil(1 << 20);
    if mib > MAX_WORLD_MIB {
        let why = format!(
            "the world {benchmark} is {mib} MiB, over the {MAX_WORLD_MIB} MiB a sandbox takes"
        );
        return Err(WorldNotCopied(why).into());
    }
    Ok(files
        .into_iter()
        .map(|(to, local, _)| (to, local))
        .collect())
}

/// The allowed files under `dir`, each with its path under `at` and its length.
fn listed(dir: &Path, at: &str) -> std::io::Result<Vec<(String, PathBuf, u64)>> {
    let mut out = vec![];
    walk(dir, at, &mut out)?;
    Ok(out)
}

/// True when a file named `name` may be copied.
fn copied(name: &str) -> bool {
    let a = &*ALLOWED;
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy());
    let allowed = ext.is_some_and(|e| a.extensions.iter().any(|x| *x == e))
        || a.names.iter().any(|n| n == name);
    allowed && !a.never.iter().any(|n| n == name)
}

fn walk(dir: &Path, at: &str, out: &mut Vec<(String, PathBuf, u64)>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_string();
        let (kind, to) = (e.file_type()?, format!("{at}/{name}"));
        if kind.is_dir() && !ALLOWED.skip_dirs.contains(&name) {
            walk(&e.path(), &to, out)?;
        } else if kind.is_file() && copied(&name) {
            out.push((to, e.path(), e.metadata()?.len()));
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

    /// The sandbox paths of the files under a folder holding `files`.
    fn copied_of(files: &[&str]) -> Vec<String> {
        let dir = std::env::temp_dir().join(format!("shaked-walk-{}", uuid::Uuid::new_v4()));
        for f in files {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        let mut at: Vec<String> = listed(&dir.join("w"), "/s/w")
            .unwrap()
            .into_iter()
            .map(|f| f.0)
            .collect();
        at.sort();
        std::fs::remove_dir_all(dir).unwrap();
        at
    }

    #[test]
    fn only_what_a_world_needs_is_copied() {
        let files = [
            "w/world.json",
            "w/shakedown-package.toml",
            "w/svc/app.py",
            "w/svc/Dockerfile",
            "w/.venv/lib.py",
            "w/__pycache__/c.py",
            "w/data/rows.csv",
            "w/notes.bin",
        ];
        let want = [
            "/s/w/data/rows.csv",
            "/s/w/shakedown-package.toml",
            "/s/w/svc/Dockerfile",
            "/s/w/svc/app.py",
            "/s/w/world.json",
        ];
        assert_eq!(copied_of(&files), want);
    }

    #[test]
    fn secrets_and_keys_are_never_copied() {
        let files = [
            "w/secrets.json",
            "w/tls/server.pem",
            "w/id.key",
            "w/vault.key",
            "w/svc/.env",
            "w/world.json",
        ];
        assert_eq!(copied_of(&files), ["/s/w/world.json"]);
    }
}
