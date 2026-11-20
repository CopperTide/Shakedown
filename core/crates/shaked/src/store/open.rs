//! Opening the metadata database: the roles, the migrations of coppertide-schema, the local seed,
//! and the connection pools.

use anyhow::{bail, Context, Result};
use coppertide_schema::{migrate, Plan, Stream, BOOTSTRAP_SQL};
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::{Client, NoTls};

use crate::pg::Admin;

/// Every pooled connection's search path: shaked's tables, then who people are.
pub(crate) const SEARCH_PATH: &str = "shakedown, identity, authz, public";
/// Who applied a migration, in `schema_meta.migrations`.
const APPLIED_BY: &str = concat!("shaked ", env!("CARGO_PKG_VERSION"));
const POOL_SIZE: usize = 16;

/// What a login that is not a superuser needs first. Never names the login's URL.
const BOOTSTRAP_HINT: &str = "this Postgres login is not a superuser and is not in the roles \
     shaked_app and shaked_system: run core/crates/coppertide-schema/bootstrap.sql once as a \
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
    migrate(c, &plan(&[Stream::Identity]))
        .await
        .context("migrating stream identity")?;
    super::seed::seed_local(c)
        .await
        .context("seeding the local owner")?;
    migrate(c, &plan(&[Stream::Shakedown]))
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
    let mut all: Vec<u8> = coppertide_schema::migrations()
        .iter()
        .flat_map(|m| m.checksum)
        .collect();
    all.extend_from_slice(super::seed::SEED_SQL.as_bytes());
    let d = ring::digest::digest(&ring::digest::SHA256, &all);
    d.as_ref()[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// A hosted shaked's three Postgres logins: requests (in `shaked_app`), system jobs (in
/// `shaked_system`) and migrations (`coppertide_migrator`, or a superuser). Owner decision D15:
/// the database is the Cloud gateway's, so stream identity is the gateway's to apply.
#[derive(Clone)]
pub struct DbUrls {
    /// `SHAKEDOWN_PG_URL`.
    pub app: String,
    /// `SHAKEDOWN_SYSTEM_PG_URL`.
    pub system: Option<String>,
    /// `SHAKEDOWN_MIGRATE_PG_URL`.
    pub migrate: Option<String>,
}

impl DbUrls {
    /// The three settings; a hosted shaked needs every one.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> DbUrls {
        DbUrls {
            app: get("SHAKEDOWN_PG_URL").unwrap_or_default(),
            system: get("SHAKEDOWN_SYSTEM_PG_URL"),
            migrate: get("SHAKEDOWN_MIGRATE_PG_URL"),
        }
    }
}

/// What a hosted shaked needs of the database: stream identity at 2 (applied by the gateway).
const HOSTED_PLAN: Plan<'static> = Plan {
    apply: &[Stream::Shakedown],
    require: &[(Stream::Identity, 2)],
    applied_by: APPLIED_BY,
};
/// How often and how long a hosted start waits for the gateway's identity stream.
const READY_EVERY: std::time::Duration = std::time::Duration::from_secs(5);
const READY_GIVE_UP: std::time::Duration = std::time::Duration::from_secs(120);

/// Applies stream shakedown over the migrate login, waiting for stream identity when the gateway
/// is a deploy behind. Never seeds the local owner.
pub(super) async fn migrate_hosted(url: &str) -> Result<()> {
    let (mut c, conn) = tokio_postgres::connect(url, NoTls)
        .await
        .context("connecting to migrate")?;
    let task = tokio::spawn(conn);
    let mut done = migrate(&mut c, &HOSTED_PLAN).await;
    if let Err(coppertide_schema::Error::Behind { .. }) = done {
        let ready =
            coppertide_schema::wait_ready(&mut c, &HOSTED_PLAN, READY_EVERY, READY_GIVE_UP).await;
        done = match ready {
            Ok(_) | Err(coppertide_schema::Error::Behind { .. }) => {
                migrate(&mut c, &HOSTED_PLAN).await
            }
            Err(e) => Err(e),
        };
    }
    drop(c);
    let _ = task.await;
    done.map(drop).context("migrating stream shakedown")
}

/// A hosted request login must be under row-level security: not a superuser, not BYPASSRLS, and in
/// `shaked_app` (fail closed).
pub(super) async fn refuse_bypassing_login(app: &Pool) -> Result<()> {
    let c = app
        .get()
        .await
        .context("connecting with SHAKEDOWN_PG_URL")?;
    let sql = "SELECT rolsuper OR rolbypassrls,
                      CASE WHEN to_regrole('shaked_app') IS NULL THEN false ELSE pg_has_role('shaked_app', 'MEMBER') END
                 FROM pg_roles WHERE rolname = current_user";
    let row = c.query_one(sql, &[]).await?;
    let (bypasses, member): (bool, bool) = (row.get(0), row.get(1));
    if bypasses || !member {
        bail!("SHAKEDOWN_PG_URL's login must be in shaked_app and under row-level security (not a superuser, not BYPASSRLS)");
    }
    Ok(())
}
