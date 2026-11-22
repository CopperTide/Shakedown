//! The credential an `http` agent's endpoint is sent: read when a request is made, for the caller
//! the run is for, from one of that caller's own stored keys (or, where a layer lets it, from a
//! `SHAKEDOWN_AGENT_*` environment variable). A team keeps only the reference ([`AuthRef`]).

use crate::{
    app::AppState,
    auth::Caller,
    layer::Policy,
    model::AuthRef,
    store::{KeyKind, KeyRow},
};

/// The only environment variables an agent may be sent.
pub const ENV_PREFIX: &str = "SHAKEDOWN_AGENT_";

/// Why an environment variable may not be sent, if it may not. It never says whether the
/// variable exists.
pub fn env_refusal(policy: &Policy, var: &str) -> Option<String> {
    if !policy.environment_keys {
        return Some("environment variables are not read here: use a stored key".into());
    }
    let allowed = var.starts_with(ENV_PREFIX) && var.len() > ENV_PREFIX.len();
    (!allowed).then(|| format!("only {ENV_PREFIX}* variables may be sent to an agent"))
}

/// Why `row`, one of the caller's keys, may not be an endpoint's credential, if it may not.
pub fn key_refusal(row: &KeyRow, own: bool) -> Option<String> {
    let name = &row.name;
    if !own {
        return Some(format!(
            "the key {name:?} is shared with you: an endpoint can be sent only a key of your own"
        ));
    }
    (row.kind != KeyKind::Custom).then(|| {
        format!("the key {name:?} is a model provider key: store the endpoint's credential as a custom key")
    })
}

fn from_env(policy: &Policy, var: &str) -> Result<String, String> {
    if let Some(why) = env_refusal(policy, var) {
        return Err(why);
    }
    let value = std::env::var(var).map_err(|_| format!("the variable {var} is not set"))?;
    non_empty(var, value)
}

fn non_empty(name: &str, secret: String) -> Result<String, String> {
    let secret = secret.trim();
    match secret.is_empty() {
        true => Err(format!("the credential {name:?} is empty")),
        false => Ok(secret.to_string()),
    }
}

async fn from_key(state: &AppState, caller: &Caller, name: &str) -> Result<String, String> {
    let io = |e: anyhow::Error| format!("{e:#}");
    let records = state.store.at(caller);
    let rows = records.list_keys().await.map_err(io)?;
    let row = rows.iter().find(|r| r.name == name);
    let Some(row) = row else {
        return Err(format!("no key named {name:?} is stored"));
    };
    let mine = state.store.layer().own_keys(caller, &[row.id]).await;
    let own = mine.map_err(io)?.contains(&row.id);
    if let Some(why) = key_refusal(row, own) {
        return Err(why);
    }
    let vault = state.vault()?;
    let open = records.open_key(&vault, row.id).await.map_err(io)?;
    let open = open.ok_or_else(|| format!("no key named {name:?} is stored"))?;
    non_empty(name, open.secret_text().to_string())
}

/// The secret `auth` stands for, for `caller`. It is the caller of the run that asks, so a
/// reference cited by another caller finds only that caller's own keys.
pub async fn resolve(state: &AppState, caller: &Caller, auth: &AuthRef) -> Result<String, String> {
    match auth {
        AuthRef::Env(var) => from_env(state.store.layer().policy(), var),
        AuthRef::Key(name) => from_key(state, caller, name).await,
    }
}

/// The `Authorization` header value an endpoint is sent for `auth`.
pub async fn bearer(state: &AppState, caller: &Caller, auth: &AuthRef) -> Result<String, String> {
    let secret = resolve(state, caller, auth).await?;
    Ok(format!("Bearer {secret}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(environment_keys: bool) -> Policy {
        Policy {
            serves_others: !environment_keys,
            private_model_urls: environment_keys,
            runs_packages_here: environment_keys,
            environment_keys,
        }
    }

    #[test]
    fn a_layer_that_serves_others_reads_no_variable() {
        for var in ["SHAKEDOWN_VAULT_KEY", "SHAKEDOWN_AGENT_TOKEN", "PATH"] {
            let why = env_refusal(&policy(false), var).unwrap();
            assert!(why.contains("not read here") && !why.contains(var), "{why}");
        }
    }

    #[test]
    fn locally_only_agent_variables_may_be_sent() {
        let local = policy(true);
        assert_eq!(env_refusal(&local, "SHAKEDOWN_AGENT_TOKEN"), None);
        for var in [
            "SHAKEDOWN_VAULT_KEY",
            "SHAKEDOWN_PG_URL",
            "OPENROUTER_API_KEY",
            "SHAKEDOWN_AGENT_",
        ] {
            let why = env_refusal(&local, var).unwrap();
            assert!(why.contains("SHAKEDOWN_AGENT_"), "{why}");
        }
        assert!(from_env(&local, "SHAKEDOWN_VAULT_KEY").is_err());
    }
}
