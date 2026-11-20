//! Opening the metadata database: the roles, the migrations of shakedown-schema, the local seed,
//! and the connection pools.

use anyhow::{bail, Context, Result};
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use shakedown_schema::{base, migrate, retiring_identity, Plan, Stream, BOOTSTRAP_SQL, SHAKEDOWN};
use tokio_postgres::{Client, NoTls};

use crate::pg::Admin;

/// Every pooled connection's search path: shaked's tables, then who people are.
pub(crate) const SEARCH_PATH: &str = "shakedown, identity, authz, public";
/// Who applied a migration, in `schema_meta.migrations`.
const APPLIED_BY: &str = concat!("shaked ", env!("CARGO_PKG_VERSION"));
const POOL_SIZE: usize = 16;

/// What a login that is not a superuser needs first. Never names the login's URL.
const BOOTSTRAP_HINT: &str = "this Postgres login is not a superuser and is not in the roles \
     shaked_app and shaked_system: run core/crates/shakedown-schema/bootstrap.sql once as a \
     superuser, then GRANT shaked_app, shaked_system TO the login";

/// Brings database `name` up to date for a local shaked, on one connection it closes before
/// returning: the bootstrap roles on a superuser login (else checks them), stream `identity`, the
/// local owner, stream `shakedown`, and the database's default search path.
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
    roles(c).await?;
    let plan = |s: &'static [Stream]| Plan {
        apply: s,
        require: &[],
        applied_by: APPLIED_BY,
    };
    migrate(c, &plan(&[Stream("identity")]), retiring_identity())
        .await
        .context("migrating stream identity")?;
    super::seed::seed_local(c)
        .await
        .context("seeding the local owner")?;
    migrate(c, &plan(&[SHAKEDOWN]), base())
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

/// A superuser runs the bootstrap itself; any other login must already be in both roles.
async fn roles(c: &Client) -> Result<()> {
    let sql = "SELECT rolsuper,
                      CASE WHEN to_regrole('shaked_app') IS NULL OR to_regrole('shaked_system') IS NULL
                           THEN false
                           ELSE pg_has_role('shaked_app', 'MEMBER') AND pg_has_role('shaked_system', 'MEMBER')
                      END
                 FROM pg_roles WHERE rolname = current_user";
    let row = c.query_one(sql, &[]).await?;
    let (superuser, member): (bool, bool) = (row.get(0), row.get(1));
    if superuser {
        c.batch_execute(BOOTSTRAP_SQL)
            .await
            .context("running bootstrap.sql")?;
    } else if !member {
        bail!("{BOOTSTRAP_HINT}");
    }
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

/// What a test template database is named after: the embedded migrations and the local seed, so a
/// changed schema gets a new template. 8 hex digits.
pub fn template_key() -> String {
    let mut all: Vec<u8> = shakedown_schema::retiring_identity()
        .iter()
        .chain(base())
        .flat_map(|m| m.checksum)
        .collect();
    all.extend_from_slice(super::seed::SEED_SQL.as_bytes());
    let d = ring::digest::digest(&ring::digest::SHA256, &all);
    d.as_ref()[..4].iter().map(|b| format!("{b:02x}")).collect()
}
