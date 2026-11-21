//! A small Postgres admin helper for the store's own database: create it when missing, connect
//! to it. Tests also use it to create and drop the databases they make.

use anyhow::{Context, Result};
use tokio::sync::OnceCell;
use tokio_postgres::{Client, NoTls};

/// The server shaked uses when `SHAKEDOWN_PG_URL` is not set (`deploy/compose.yml`'s).
pub const DEFAULT_URL: &str = "postgres://shakedown:shakedown@127.0.0.1:5544/postgres";
/// The default until 2026-11-19, when `deploy/compose.yml` made the user `arena`.
pub const OLD_DEFAULT_URL: &str = "postgres://arena:arena@127.0.0.1:5544/postgres";

/// `SHAKEDOWN_PG_URL`, else [`DEFAULT_URL`], else [`OLD_DEFAULT_URL`] when only that one connects
/// (a server made before the rename keeps working without a setting; said once on stderr).
pub async fn configured_url() -> String {
    let set = parts::settings::var("SHAKEDOWN_PG_URL").ok();
    pick_url(set, |url| async move { Admin::connect(url).await.is_ok() }).await
}

/// `set`, else the default that `connects`, the new one first (the new one when neither does).
async fn pick_url<F, Fut>(set: Option<String>, connects: F) -> String
where
    F: Fn(&'static str) -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    if let Some(url) = set {
        return url;
    }
    if !connects(DEFAULT_URL).await && connects(OLD_DEFAULT_URL).await {
        eprintln!(
            "shaked: Postgres user arena (the default until 2026-11-19); see README.md, Old names"
        );
        return OLD_DEFAULT_URL.to_string();
    }
    DEFAULT_URL.to_string()
}

/// Administrative handle on a Postgres server (connected to its `postgres` database on first use).
pub struct Admin {
    base_url: String,
    client: OnceCell<Client>,
}

impl Admin {
    /// `url` is a server URL such as `postgres://shakedown:shakedown@127.0.0.1:5544` (a trailing
    /// `/dbname` and query string are ignored). Connects right away.
    pub async fn connect(url: &str) -> Result<Admin> {
        let admin = Admin::lazy(url);
        admin.client().await?;
        Ok(admin)
    }

    /// Like `connect`, but connects on first use.
    pub fn lazy(url: &str) -> Admin {
        Admin {
            base_url: strip_db(url),
            client: OnceCell::new(),
        }
    }

    async fn client(&self) -> Result<&Client> {
        self.client
            .get_or_try_init(|| async {
                let (client, conn) = tokio_postgres::connect(&self.db_url("postgres"), NoTls)
                    .await
                    .with_context(|| format!("connecting to {}", self.base_url))?;
                tokio::spawn(conn);
                Ok::<_, anyhow::Error>(client)
            })
            .await
    }

    /// The URL of database `db` on this server.
    pub fn db_url(&self, db: &str) -> String {
        format!("{}/{}", self.base_url, db)
    }

    pub async fn exists(&self, name: &str) -> Result<bool> {
        let sql = "SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)";
        let row = self.client().await?.query_one(sql, &[&name]).await?;
        Ok(row.get(0))
    }

    pub async fn create_db_from(&self, new: &str, template: &str) -> Result<()> {
        let sql = format!(
            "CREATE DATABASE {} TEMPLATE {}",
            quote(new),
            quote(template)
        );
        self.client().await?.batch_execute(&sql).await?;
        Ok(())
    }

    pub async fn drop_db(&self, name: &str) -> Result<()> {
        let sql = format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", quote(name));
        self.client().await?.batch_execute(&sql).await?;
        Ok(())
    }
}

fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// The server part of a URL: scheme, credentials, host and port.
fn strip_db(url: &str) -> String {
    let url = url.split('?').next().unwrap_or(url).trim_end_matches('/');
    match url.find("://") {
        Some(i) => match url[i + 3..].find('/') {
            Some(j) => url[..i + 3 + j].to_string(),
            None => url.to_string(),
        },
        None => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{pick_url, strip_db, DEFAULT_URL, OLD_DEFAULT_URL};

    #[tokio::test]
    async fn the_default_server_is_the_new_user_else_the_old_one() {
        let only = |ok: &'static str| move |url: &'static str| async move { url == ok };
        let mine = "postgres://me@h/postgres".to_string();
        assert_eq!(pick_url(Some(mine.clone()), only(DEFAULT_URL)).await, mine);
        assert_eq!(pick_url(None, only(DEFAULT_URL)).await, DEFAULT_URL);
        assert_eq!(pick_url(None, only(OLD_DEFAULT_URL)).await, OLD_DEFAULT_URL);
        assert_eq!(pick_url(None, only("nothing")).await, DEFAULT_URL);
    }

    #[test]
    fn server_part_of_urls() {
        let base = "postgres://a:b@h:5544";
        assert_eq!(strip_db(base), base);
        assert_eq!(strip_db(&format!("{base}/postgres")), base);
        assert_eq!(strip_db(&format!("{base}/postgres?shakedown_ns=x_")), base);
        assert_eq!(strip_db(&format!("{base}/")), base);
    }
}
