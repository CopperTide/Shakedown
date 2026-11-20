//! The migrator: applies the streams a service owns, under one advisory lock per stream, checks
//! the checksums of what a database applied and the versions of the streams it reads.

use std::{collections::BTreeMap, future::Future, time::Duration};

use crate::stream::{hex, Migration, Stream};

/// What went wrong. Messages name streams and versions, never connection strings.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the database's {} {version} ({name}) differs from this binary's file: it was edited after it was applied", stream.as_str())]
    Checksum {
        stream: Stream,
        version: u32,
        name: String,
    },
    #[error("the database's {} stream is at {have}, and this needs {need}", stream.as_str())]
    Behind {
        stream: Stream,
        have: u32,
        need: u32,
    },
    #[error("migration file {0}")]
    BadFile(String),
    #[error("{0}")]
    Sql(String),
}

/// A database connection the migrator can use: the Postgres driver's side.
pub trait Conn: Send {
    /// Runs `sql`, which may hold many statements, with the simple-query protocol.
    fn batch(&mut self, sql: &str) -> impl Future<Output = Result<(), Error>> + Send;
    /// Every row of `schema_meta.migrations`; empty when that table does not exist yet.
    fn applied(&mut self) -> impl Future<Output = Result<Vec<Applied>, Error>> + Send;
}

/// One applied migration, as `schema_meta.migrations` has it.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    pub stream: Stream,
    pub version: u32,
    pub checksum: Vec<u8>,
}

/// What a service migrates: the streams it owns, the versions of others it reads, and who it is
/// (`shaked <sha>`, `sculptured-gateway <sha>`; written into the history).
#[derive(Debug, Clone, Copy)]
pub struct Plan<'a> {
    pub apply: &'a [Stream],
    pub require: &'a [(Stream, u32)],
    pub applied_by: &'a str,
}

/// What a migration did: the versions it applied, and versions in the database newer than this
/// binary knows (fine: expand-only; the caller logs them).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub applied: Vec<(Stream, u32)>,
    pub newer: Vec<(Stream, u32)>,
}

/// The history table. Run between its own lock and unlock batches: one multi-statement batch is a
/// single implicit transaction, which would release the lock before its DDL commits.
const META_DDL: &str = "CREATE SCHEMA IF NOT EXISTS schema_meta;
CREATE TABLE IF NOT EXISTS schema_meta.migrations (
  stream        text        NOT NULL CHECK (stream ~ '^[a-z][a-z_]{0,31}$'),
  version       integer     NOT NULL CHECK (version > 0),
  name          text        NOT NULL,
  checksum      bytea       NOT NULL,
  applied_at    timestamptz NOT NULL DEFAULT now(),
  applied_by    text        NOT NULL,
  execution_ms  integer     NOT NULL DEFAULT 0,
  PRIMARY KEY (stream, version)
);";

/// Applies the migrations of `set` in the streams of `plan.apply` that the database lacks, after
/// checking that `plan.require` holds. `set` is what the binary embeds ([`crate::base`], or
/// [`crate::load`] over the base and its own streams).
pub async fn migrate(
    c: &mut impl Conn,
    plan: &Plan<'_>,
    set: &[Migration],
) -> Result<Report, Error> {
    valid_applied_by(plan.applied_by)?;
    lock(c, "meta").await?;
    let made = c.batch(META_DDL).await;
    unlock(c, "meta", made).await?;
    let before = c.applied().await?;
    verify(set, &before, &streams_of(plan))?;
    requirements(&before, plan.require)?;
    let mut report = Report::default();
    for s in plan.apply {
        report.applied.extend(apply_stream(c, *s, plan, set).await?);
    }
    report.newer = newer(set, &c.applied().await?, &streams_of(plan));
    Ok(report)
}

/// Checks without writing anything: checksums match, required streams are new enough, and owned
/// streams are fully applied (else [`Error::Behind`]).
pub async fn check(c: &mut impl Conn, plan: &Plan<'_>, set: &[Migration]) -> Result<Report, Error> {
    let applied = c.applied().await?;
    verify(set, &applied, &streams_of(plan))?;
    requirements(&applied, plan.require)?;
    for s in plan.apply {
        let need = latest(set, *s);
        let missing = set.iter().any(|m| {
            m.stream == *s
                && !applied
                    .iter()
                    .any(|a| (a.stream, a.version) == (*s, m.version))
        });
        if missing {
            return Err(Error::Behind {
                stream: *s,
                have: version_of(&applied, *s),
                need,
            });
        }
    }
    Ok(Report {
        applied: Vec::new(),
        newer: newer(set, &applied, &streams_of(plan)),
    })
}

/// `check` every `every` until it is not [`Error::Behind`], giving up after `give_up` (the other
/// service may be a deploy behind).
pub async fn wait_ready(
    c: &mut impl Conn,
    plan: &Plan<'_>,
    set: &[Migration],
    every: Duration,
    give_up: Duration,
) -> Result<Report, Error> {
    let started = tokio::time::Instant::now();
    loop {
        match check(c, plan, set).await {
            Err(Error::Behind { .. }) if started.elapsed() < give_up => {
                tokio::time::sleep(every).await;
            }
            other => return other,
        }
    }
}

/// Applies the missing versions of stream `s` under its advisory lock.
async fn apply_stream(
    c: &mut impl Conn,
    s: Stream,
    plan: &Plan<'_>,
    set: &[Migration],
) -> Result<Vec<(Stream, u32)>, Error> {
    lock(c, s.as_str()).await?;
    let result = apply_locked(c, s, plan, set).await;
    unlock(c, s.as_str(), result).await
}

/// Takes the session advisory lock `coppertide-schema:<name>`.
async fn lock(c: &mut impl Conn, name: &str) -> Result<(), Error> {
    c.batch(&format!(
        "SELECT pg_advisory_lock(hashtext('coppertide-schema:{name}'))"
    ))
    .await
}

/// Releases the lock `lock` took, whatever `result` is, and passes `result` on.
async fn unlock<T>(c: &mut impl Conn, name: &str, result: Result<T, Error>) -> Result<T, Error> {
    let unlocked = c
        .batch(&format!(
            "SELECT pg_advisory_unlock(hashtext('coppertide-schema:{name}'))"
        ))
        .await;
    let done = result?;
    unlocked?;
    Ok(done)
}

async fn apply_locked(
    c: &mut impl Conn,
    s: Stream,
    plan: &Plan<'_>,
    set: &[Migration],
) -> Result<Vec<(Stream, u32)>, Error> {
    let applied = c.applied().await?;
    verify(set, &applied, &[s])?;
    let mut done = Vec::new();
    for m in set.iter().filter(|m| m.stream == s) {
        if applied
            .iter()
            .any(|a| (a.stream, a.version) == (s, m.version))
        {
            continue;
        }
        requirements(&c.applied().await?, &m.requires)?;
        run_one(c, m, plan.applied_by).await?;
        done.push((s, m.version));
    }
    Ok(done)
}

/// The INSERT that records `m` in the history, in the same transaction as `m`.
fn record_sql(m: &Migration, applied_by: &str) -> String {
    format!(
        "INSERT INTO schema_meta.migrations (stream, version, name, checksum, applied_by, execution_ms)
         VALUES ('{}', {}, '{}', decode('{}', 'hex'), '{applied_by}',
                 (extract(epoch FROM clock_timestamp() - now()) * 1000)::int);",
        m.stream.as_str(),
        m.version,
        m.name,
        hex(&m.checksum),
    )
}

/// Runs one file and records it; a failed transaction is rolled back so the connection stays usable.
async fn run_one(c: &mut impl Conn, m: &Migration, applied_by: &str) -> Result<(), Error> {
    let record = record_sql(m, applied_by);
    let what = |e: Error| {
        Error::Sql(format!(
            "{} {} ({}): {e}",
            m.stream.as_str(),
            m.version,
            m.name
        ))
    };
    if m.no_transaction {
        c.batch(m.sql).await.map_err(what)?;
        return c.batch(&record).await.map_err(what);
    }
    let sql = format!(
        "BEGIN;\nSET LOCAL lock_timeout = '5s';\nSET LOCAL statement_timeout = '60s';\n{}\n;\n{record}\nCOMMIT;",
        m.sql
    );
    if let Err(e) = c.batch(&sql).await {
        let _ = c.batch("ROLLBACK").await;
        return Err(what(e));
    }
    Ok(())
}

/// `applied_by` is formatted into SQL, so it is held to a plain alphabet.
fn valid_applied_by(s: &str) -> Result<(), Error> {
    let ok = (1..=80).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b" ._-".contains(&b));
    if ok {
        Ok(())
    } else {
        Err(Error::BadFile(format!(
            "applied_by {s:?} must match ^[a-z0-9 ._-]{{1,80}}$"
        )))
    }
}

/// The streams a plan touches: those it applies and those it reads.
fn streams_of(plan: &Plan<'_>) -> Vec<Stream> {
    let mut s: Vec<Stream> = plan.apply.to_vec();
    s.extend(plan.require.iter().map(|(r, _)| *r));
    s.sort();
    s.dedup();
    s
}

/// Refuses a stored checksum of `streams` that the embedded file neither has nor also accepts.
fn verify(set: &[Migration], applied: &[Applied], streams: &[Stream]) -> Result<(), Error> {
    let known: BTreeMap<(Stream, u32), &Migration> =
        set.iter().map(|m| ((m.stream, m.version), m)).collect();
    for a in applied.iter().filter(|a| streams.contains(&a.stream)) {
        if let Some(m) = known.get(&(a.stream, a.version)) {
            if !m.accepts(&a.checksum) {
                return Err(Error::Checksum {
                    stream: a.stream,
                    version: a.version,
                    name: m.name.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// [`Error::Behind`] for the first (stream, version) the database has not reached.
fn requirements(applied: &[Applied], require: &[(Stream, u32)]) -> Result<(), Error> {
    for (s, need) in require {
        let have = version_of(applied, *s);
        if have < *need {
            return Err(Error::Behind {
                stream: *s,
                have,
                need: *need,
            });
        }
    }
    Ok(())
}

/// The highest applied version of `s`, 0 for none.
fn version_of(applied: &[Applied], s: Stream) -> u32 {
    applied
        .iter()
        .filter(|a| a.stream == s)
        .map(|a| a.version)
        .max()
        .unwrap_or(0)
}

fn latest(set: &[Migration], s: Stream) -> u32 {
    set.iter()
        .filter(|m| m.stream == s)
        .map(|m| m.version)
        .max()
        .unwrap_or(0)
}

/// Applied versions above the newest this binary embeds: of `streams`, and of every stream `set`
/// does not know (another service's, which this binary never touches).
fn newer(set: &[Migration], applied: &[Applied], streams: &[Stream]) -> Vec<(Stream, u32)> {
    let known = |s: Stream| set.iter().any(|m| m.stream == s);
    let mut n: Vec<(Stream, u32)> = applied
        .iter()
        .filter(|a| streams.contains(&a.stream) || !known(a.stream))
        .filter(|a| a.version > latest(set, a.stream))
        .map(|a| (a.stream, a.version))
        .collect();
    n.sort();
    n
}
