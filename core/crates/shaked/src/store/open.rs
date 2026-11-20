//! Opening the metadata database: the base migrations of shakedown-schema and the connection pool.

use anyhow::{bail, Context, Result};
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use shakedown_schema::{base, migrate, retired_in, Conn, Plan, SHAKEDOWN};
use tokio_postgres::{Client, NoTls};

use crate::pg::Admin;

/// Every pooled connection's search path: shaked's tables first.
pub(crate) const SEARCH_PATH: &str = "shakedown, public";
/// Who applied a migration, in `schema_meta.migrations`.
const APPLIED_BY: &str = concat!("shaked ", env!("CARGO_PKG_VERSION"));
const POOL_SIZE: usize = 16;

/// A database an unreleased build migrated. Names no connection string.
const RETIRED_HINT: &str = "this database was migrated by an unreleased factory build of shaked \
     (2026-11-20); convert it with core/scripts/o1-retire-factory-orgs.sql, as \
     docs/superpowers/plans/2026-11-20-hosted-m1-foundation.md, step O1, says, or point \
     SHAKEDOWN_DB at another database";

/// Brings database `name` up to date, on one connection it closes before returning: stream
/// `shakedown` and the database's default search path. A database from the retired factory build
/// is refused before anything is written.
pub async fn prepare_db(admin: &Admin, name: &str) -> Result<()> {
    let (mut client, conn) = tokio_postgres::connect(&admin.db_url(name), NoTls)
        .await
        .with_context(|| format!("connecting to {name}"))?;
    let task = tokio::spawn(conn);
    let done = prepare_on(&mut client, name).await;
    drop(client);
    let _ = task.await;
    done
}

async fn prepare_on(c: &mut Client, name: &str) -> Result<()> {
    let applied = Conn::applied(c)
        .await
        .context("reading the migration history")?;
    if retired_in(&applied) {
        bail!("{RETIRED_HINT}");
    }
    let plan = Plan {
        apply: &[SHAKEDOWN],
        require: &[],
        applied_by: APPLIED_BY,
    };
    migrate(c, &plan, base())
        .await
        .context("migrating stream shakedown")?;
    let quoted = name.replace('"', "\"\"");
    c.batch_execute(&format!(
        "ALTER DATABASE \"{quoted}\" SET search_path = {SEARCH_PATH}"
    ))
    .await
    .context("setting the database's search path")?;
    Ok(())
}

/// A pool on `url` whose connections use [`SEARCH_PATH`].
pub(super) fn pool(url: &str) -> Result<Pool> {
    let mut cfg: tokio_postgres::Config = url.parse().context("the Postgres URL")?;
    cfg.options(format!("-c search_path={}", SEARCH_PATH.replace(' ', "")));
    let mgr = Manager::from_config(
        cfg,
        NoTls,
        ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        },
    );
    Ok(Pool::builder(mgr)
        .max_size(POOL_SIZE)
        .runtime(Runtime::Tokio1)
        .build()?)
}

/// What a test template database is named after: the embedded migrations, so a changed schema gets
/// a new template. 8 hex digits.
pub fn template_key() -> String {
    let all: Vec<u8> = base().iter().flat_map(|m| m.checksum).collect();
    let d = ring::digest::digest(&ring::digest::SHA256, &all);
    d.as_ref()[..4].iter().map(|b| format!("{b:02x}")).collect()
}
