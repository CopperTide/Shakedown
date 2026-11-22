//! shaked's commands besides serving: `shaked token create`, `shaked store copy`, `shaked mcp`
//! and `shaked import`.

use anyhow::{bail, Context, Result};

use crate::{
    auth::{Caller, TokenScope},
    pg::Admin,
    store::Store,
};

/// Runs the command `args` (after the program name) names: Some(exit code) when it was one.
pub async fn run(args: &[String]) -> Option<Result<()>> {
    match args.first().map(String::as_str) {
        Some("token") => Some(token(&args[1..]).await),
        Some("store") => Some(store_copy(&args[1..]).await),
        Some("mcp") => Some(mcp(&args[1..]).await),
        Some("import") => Some(import(&args[1..]).await),
        _ => None,
    }
}

/// `mcp [--project <dir>]`: the control MCP server over stdio for the project folder `dir`
/// (default `SHAKEDOWN_PROJECT`).
async fn mcp(args: &[String]) -> Result<()> {
    let dir = match args {
        [flag, dir] if flag == "--project" => Some(std::path::PathBuf::from(dir)),
        [] => parts::settings::var_os("SHAKEDOWN_PROJECT")
            .filter(|v| !v.is_empty())
            .map(Into::into),
        _ => bail!("usage: shaked mcp [--project <folder>]"),
    };
    let Some(dir) = dir else {
        bail!("shaked mcp needs a project: pass --project <folder> or set SHAKEDOWN_PROJECT");
    };
    let project = crate::project_state::ProjectState::unopened(dir);
    crate::ops::stdio::run(std::sync::Arc::new(project)).await
}

const IMPORT_USAGE: &str =
    "usage: shaked import <git-url-or-path> [--ref <ref>] [--id <id>] [--project <folder>] [--dry-run]";

/// What `shaked import` was asked.
#[derive(Debug, PartialEq)]
struct ImportArgs {
    url: String,
    git_ref: Option<String>,
    id: Option<String>,
    project: Option<std::path::PathBuf>,
    dry_run: bool,
}

impl ImportArgs {
    fn parse(args: &[&str]) -> Result<ImportArgs> {
        let (mut url, mut git_ref, mut id, mut project, mut dry_run) =
            (None, None, None, None, false);
        let mut it = args.iter();
        while let Some(&a) = it.next() {
            let mut value = || it.next().map(|v| v.to_string()).context(IMPORT_USAGE);
            match a {
                "--ref" => git_ref = Some(value()?),
                "--id" => id = Some(value()?),
                "--project" => project = Some(value()?.into()),
                "--dry-run" => dry_run = true,
                _ if a.starts_with('-') || url.is_some() => bail!(IMPORT_USAGE),
                _ => url = Some(a.to_string()),
            }
        }
        let url = url.context(IMPORT_USAGE)?;
        Ok(ImportArgs {
            url,
            git_ref,
            id,
            project,
            dry_run,
        })
    }
}

/// A local path given relative to the working folder, made absolute; URLs stay as they are.
fn absolute_source(source: &str) -> String {
    if crate::git_import::git_url(source).is_ok() {
        return source.to_string();
    }
    match std::fs::canonicalize(source) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(_) => source.to_string(),
    }
}

/// `import <git-url-or-path> [--ref R] [--id I] [--project F] [--dry-run]`: `project.import_git`
/// on the project folder (default `SHAKEDOWN_PROJECT`), as the person at this machine.
async fn import(args: &[String]) -> Result<()> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let asked = ImportArgs::parse(&words)?;
    let dir = asked.project.clone().or_else(|| {
        parts::settings::var_os("SHAKEDOWN_PROJECT")
            .filter(|v| !v.is_empty())
            .map(Into::into)
    });
    let Some(dir) = dir else {
        bail!("shaked import needs a project folder: pass --project <folder> or set SHAKEDOWN_PROJECT");
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let project = std::sync::Arc::new(crate::project_state::ProjectState::new(Some(dir)));
    let ctx = crate::ops::OpCtx::project_only(project);
    let op = crate::ops::find("project.import_git").context("no import operation")?;
    let input = serde_json::json!({
        "url": absolute_source(&asked.url), "ref": asked.git_ref, "id": asked.id, "confirm": !asked.dry_run,
    });
    let out = (op.run)(&ctx, op, input)
        .await
        .map_err(|e| anyhow::anyhow!(e.message))?;
    print!("{}", import_summary(&out));
    Ok(())
}

/// The lists an import answers, how each entry is labelled on the terminal and the fields it
/// shows (none: the entry is text).
const SUMMARY_ROWS: [(&str, &str, &[&str]); 7] = [
    ("packages", "package", &["kind", "name", "dir"]),
    ("items", "item", &["action", "kind", "id"]),
    ("views", "view", &["world", "file"]),
    (
        "compose",
        "compose file (a service part candidate)",
        &["file"],
    ),
    ("notes", "note", &[]),
    ("problems", "problem", &[]),
    ("conflicts", "conflict", &[]),
];

/// What an import found and did, for the terminal.
fn import_summary(out: &serde_json::Value) -> String {
    let fields = |v: &serde_json::Value, keys: &[&str]| -> String {
        let words: Vec<&str> = match keys {
            [] => vec![v.as_str().unwrap_or("")],
            keys => keys.iter().map(|k| v[*k].as_str().unwrap_or("")).collect(),
        };
        words.join(" ")
    };
    let commit = out["commit"].as_str().unwrap_or("");
    let short = &commit[..commit.len().min(12)];
    let mut lines = vec![format!("{} ({short})", fields(out, &["url", "ref"]))];
    for (key, label, keys) in SUMMARY_ROWS {
        let entries = out[key].as_array().into_iter().flatten();
        lines.extend(entries.map(|v| format!("{label}: {}", fields(v, keys))));
    }
    lines.push(match out["proposed"].as_bool() {
        Some(true) => "dry run: nothing was installed or written".into(),
        _ => "installed and written; shaked registers the packages at its next start or deploy"
            .into(),
    });
    lines.join("\n") + "\n"
}

/// `token create --name <n> [--scope read|run|admin ...] [--expires-days N]`: prints a token of
/// the person at this machine, and nothing else.
async fn token(args: &[String]) -> Result<()> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let Some(("create", rest)) = words.split_first().map(|(f, r)| (*f, r)) else {
        bail!("usage: shaked token create --name <name> [--scope read|run|admin ...] [--expires-days N]");
    };
    let asked = TokenArgs::parse(rest)?;
    let store = Store::open(&Admin::lazy(&crate::pg::configured_url().await)).await?;
    let scope = Caller::person();
    let expires = asked
        .expires_days
        .map(|d| chrono::Utc::now() + chrono::Duration::days(d));
    let minted = store
        .at(&scope)
        .create_api_token(&asked.name, &asked.scopes, expires)
        .await?;
    println!("{}", minted.token);
    Ok(())
}

struct TokenArgs {
    name: String,
    scopes: Vec<TokenScope>,
    expires_days: Option<i64>,
}

impl TokenArgs {
    fn parse(args: &[&str]) -> Result<TokenArgs> {
        let (mut name, mut scopes, mut expires_days) = (None, vec![], None);
        let mut it = args.iter();
        while let Some(flag) = it.next() {
            let value = it.next().with_context(|| format!("{flag} needs a value"))?;
            match *flag {
                "--name" => name = Some(value.to_string()),
                "--scope" => scopes.push(
                    TokenScope::parse(value).with_context(|| format!("unknown scope {value}"))?,
                ),
                "--expires-days" => {
                    expires_days = Some(value.parse().context("--expires-days is a number")?)
                }
                other => bail!("unknown option {other}"),
            }
        }
        if scopes.is_empty() {
            scopes.push(TokenScope::Read);
        }
        Ok(TokenArgs {
            name: name.context("--name is needed")?,
            scopes,
            expires_days,
        })
    }
}

/// What `store copy` was asked.
#[derive(Debug, PartialEq, Eq)]
struct CopyArgs {
    from: String,
    to: String,
    prefix: Option<String>,
    repoint: bool,
}

const COPY_USAGE: &str =
    "usage: shaked store copy --from <store> --to <store> [--prefix <p>/] [--repoint]";

impl CopyArgs {
    fn parse(args: &[&str]) -> Result<CopyArgs> {
        let Some(("copy", rest)) = args.split_first().map(|(f, r)| (*f, r)) else {
            bail!(COPY_USAGE);
        };
        let (mut from, mut to, mut prefix, mut repoint) = (None, None, None, false);
        let mut it = rest.iter();
        while let Some(flag) = it.next() {
            if *flag == "--repoint" {
                repoint = true;
                continue;
            }
            let value = it.next().with_context(|| format!("{flag} needs a value"))?;
            match *flag {
                "--from" => from = Some(value.to_string()),
                "--to" => to = Some(value.to_string()),
                "--prefix" => prefix = Some(value.to_string()),
                other => bail!("unknown option {other}; {COPY_USAGE}"),
            }
        }
        let (from, to) = (from.context(COPY_USAGE)?, to.context(COPY_USAGE)?);
        Ok(CopyArgs {
            from,
            to,
            prefix,
            repoint,
        })
    }
}

/// `store copy`: copies the objects of one store to another, each verified by sha256, and with
/// `--repoint` points the saves that verified at the destination. Fails when any object failed.
async fn store_copy(args: &[String]) -> Result<()> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let asked = CopyArgs::parse(&words)?;
    let stores = crate::objects::ObjectStores::from_env()
        .map_err(anyhow::Error::msg)?
        .context("SHAKEDOWN_OBJECT_STORE names no stores")?;
    let name = |n: &str| crate::objects::StoreName::new(n).map_err(anyhow::Error::msg);
    let (from, to) = (name(&asked.from)?, name(&asked.to)?);
    let db = match asked.repoint {
        true => Some(Store::open(&Admin::lazy(&crate::pg::configured_url().await)).await?),
        false => None,
    };
    let r = crate::objects::copy(&stores, &from, &to, asked.prefix.as_deref(), db.as_ref()).await?;
    println!(
        "copied {}, already there {}, repointed {}, failed {}",
        r.copied,
        r.skipped,
        r.repointed,
        r.failed.len()
    );
    for (key, why) in &r.failed {
        eprintln!("failed: {key}: {why}");
    }
    match r.failed.is_empty() {
        true => Ok(()),
        false => bail!("{} objects did not copy", r.failed.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_reads_its_flags() {
        let asked = ImportArgs::parse(&["../repo", "--ref", "main", "--dry-run"]).unwrap();
        assert_eq!(asked.url, "../repo");
        assert_eq!(asked.git_ref.as_deref(), Some("main"));
        assert!(asked.dry_run && asked.id.is_none() && asked.project.is_none());
        assert!(ImportArgs::parse(&[]).is_err());
        assert!(ImportArgs::parse(&["a", "b"]).is_err());
        assert!(ImportArgs::parse(&["a", "--ref"]).is_err());
    }

    #[test]
    fn store_copy_reads_its_flags() {
        let a = CopyArgs::parse(&["copy", "--from", "a", "--to", "b", "--repoint"]).unwrap();
        let want = CopyArgs {
            from: "a".into(),
            to: "b".into(),
            prefix: None,
            repoint: true,
        };
        assert_eq!(a, want);
        assert!(CopyArgs::parse(&["copy", "--from", "a"]).is_err());
        assert!(CopyArgs::parse(&["move"]).is_err());
    }
}
