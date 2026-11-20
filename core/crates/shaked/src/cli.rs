//! shaked's commands besides serving: `shaked token create`.

use anyhow::{bail, Context, Result};

use crate::{
    auth::{local_owner, TokenScope},
    pg::Admin,
    store::Store,
};

/// Runs the command `args` (after the program name) names: Some(exit code) when it was one.
pub async fn run(args: &[String]) -> Option<Result<()>> {
    match args.first().map(String::as_str) {
        Some("token") => Some(token(&args[1..]).await),
        _ => None,
    }
}

/// `token create --name <n> [--scope read|run|admin ...] [--expires-days N]`: prints a token of
/// the local owner, and nothing else.
async fn token(args: &[String]) -> Result<()> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let Some(("create", rest)) = words.split_first().map(|(f, r)| (*f, r)) else {
        bail!("usage: shaked token create --name <name> [--scope read|run|admin ...] [--expires-days N]");
    };
    let asked = TokenArgs::parse(rest)?;
    let store = Store::open(&Admin::lazy(&crate::pg::configured_url().await)).await?;
    let scope = local_owner();
    let expires = asked
        .expires_days
        .map(|d| chrono::Utc::now() + chrono::Duration::days(d));
    let minted = store
        .at(&scope)
        .create_api_token(&asked.name, &asked.scopes, None, expires)
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
