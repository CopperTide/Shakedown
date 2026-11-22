//! Fetching one ref of a repo, shallow, into a folder of its own: git with hooks off, no prompts
//! and only the https, http, ssh, git and file transports.

use std::path::{Path, PathBuf};

use super::ImportError;

/// The git URL for `source`: a URL as it is, an absolute path as `file://<path>`. Other
/// transports (`ext::`, `fd::`) and relative paths are refused.
pub fn git_url(source: &str) -> Result<String, ImportError> {
    let s = source.trim();
    let schemes = ["https://", "http://", "ssh://", "git://", "file://"];
    if schemes.iter().any(|p| s.starts_with(p)) {
        return Ok(s.to_string());
    }
    if s.starts_with('/') {
        return Ok(format!("file://{s}"));
    }
    if scp_like(s) {
        return Ok(s.to_string());
    }
    Err(ImportError::Bad(format!(
        "{s:?} is not a git URL: give https://, ssh://, git@host:path, file:// or an absolute path"
    )))
}

/// `user@host:path`, git's scp-like ssh form.
fn scp_like(s: &str) -> bool {
    let Some((user_host, path)) = s.split_once(':') else {
        return false;
    };
    let Some((user, host)) = user_host.split_once('@') else {
        return false;
    };
    let word = |w: &str| {
        !w.is_empty()
            && w.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    word(user) && word(host) && !path.is_empty() && !path.starts_with("//")
}

/// The import id a source gives by default: its repo's name, lowercased, `.git` dropped.
pub fn default_id(source: &str) -> String {
    let s = source.trim().trim_end_matches('/');
    let last = s.rsplit(['/', ':']).next().unwrap_or(s);
    let last = last.strip_suffix(".git").unwrap_or(last);
    let mut id: String = last
        .chars()
        .map(|c| match c.to_ascii_lowercase() {
            c @ ('a'..='z' | '0'..='9') => c,
            _ => '-',
        })
        .collect();
    id = id.trim_matches('-').to_string();
    id.truncate(40);
    if id.is_empty() {
        "repo".into()
    } else {
        id
    }
}

/// A fetched checkout and the commit it is at.
pub struct Fetched {
    pub dir: PathBuf,
    pub commit: String,
}

/// Fetches `git_ref` of `url` with depth 1 into `<into>/<commit12>` (kept as it is when that
/// commit was fetched before).
pub async fn fetch(url: &str, git_ref: &str, into: &Path) -> Result<Fetched, ImportError> {
    std::fs::create_dir_all(into).map_err(|e| ImportError::io("creating", into, e))?;
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let tmp = into.join(format!(".fetch-{}", &hex[..8]));
    let fetched = checkout(url, git_ref, &tmp).await;
    let moved = fetched.and_then(|commit| {
        let dir = into.join(short(&commit));
        match dir.exists() {
            true => Ok(Fetched { dir, commit }),
            false => std::fs::rename(&tmp, &dir)
                .map(|()| Fetched {
                    dir: dir.clone(),
                    commit,
                })
                .map_err(|e| ImportError::io("moving the checkout to", &dir, e)),
        }
    });
    if tmp.exists() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    moved
}

/// Checks `git_ref` of `url` out into the new folder `dir`, answering its commit.
async fn checkout(url: &str, git_ref: &str, dir: &Path) -> Result<String, ImportError> {
    git(None, &["init", "-q", &dir.to_string_lossy()]).await?;
    git(Some(dir), &["remote", "add", "origin", url]).await?;
    let fetch = [
        "fetch",
        "-q",
        "--depth",
        "1",
        "--no-tags",
        "origin",
        git_ref,
    ];
    git(Some(dir), &fetch).await?;
    git(Some(dir), &["checkout", "-q", "--detach", "FETCH_HEAD"]).await?;
    Ok(git(Some(dir), &["rev-parse", "HEAD"])
        .await?
        .trim()
        .to_string())
}

/// The first 12 characters of a commit, its folder's name.
pub fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

/// The transports git may use; every other one (`ext::`, `fd::`) is off.
const ALLOWED: [&str; 5] = ["https", "http", "ssh", "git", "file"];

/// Runs git (in `dir`) with hooks off and no prompts, answering its standard output.
async fn git(dir: Option<&Path>, args: &[&str]) -> Result<String, ImportError> {
    let mut cmd = tokio::process::Command::new("git");
    if let Some(dir) = dir {
        cmd.arg("-C").arg(dir);
    }
    cmd.args([
        "-c",
        "protocol.allow=never",
        "-c",
        "core.hooksPath=/dev/null",
    ]);
    for t in ALLOWED {
        cmd.arg("-c").arg(format!("protocol.{t}.allow=always"));
    }
    cmd.args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null());
    let out = cmd
        .output()
        .await
        .map_err(|e| ImportError::Failed(format!("running git: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let what = args.first().copied().unwrap_or("git");
        return Err(ImportError::Failed(format!("git {what}: {}", err.trim())));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_paths_are_taken_and_other_transports_refused() {
        assert_eq!(git_url("/srv/repo").unwrap(), "file:///srv/repo");
        for ok in [
            "https://github.com/CopperTide/x.git",
            "ssh://git@host/x",
            "file:///tmp/x",
            "git@github.com:CopperTide/x.git",
        ] {
            assert_eq!(git_url(ok).unwrap(), ok);
        }
        for bad in [
            "ext::sh -c touch% /tmp/pwned",
            "fd::17",
            "relative/path",
            "",
        ] {
            assert!(git_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_default_id_is_the_repo_name() {
        assert_eq!(
            default_id("https://github.com/Acme/Airline-World.git"),
            "airline-world"
        );
        assert_eq!(default_id("/home/me/airline_world/"), "airline-world");
        assert_eq!(default_id("git@github.com:acme/ops.git"), "ops");
    }
}
