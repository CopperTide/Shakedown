//! The migrator against a real Postgres (the compose server, a scratch database per test).

mod support;

use std::time::Duration;

use shakedown_schema::{
    base, check, load, migrate, wait_ready, Error, Files, Migration, Plan, Stream, BASE_FILES,
    SHAKEDOWN,
};
use support::{count, fingerprint, Scratch};

/// Every migration this crate embeds: the base.
fn all() -> Vec<Migration> {
    base().to_vec()
}

fn migrations() -> Vec<Migration> {
    all()
}

const LOCAL: Plan<'static> = Plan {
    apply: &[SHAKEDOWN],
    require: &[],
    applied_by: "test",
};

/// A row in each of today's shaked tables, in `public`.
const TODAYS_ROWS: &str = "
    INSERT INTO runs (id, task_id, team) VALUES ('r1', 't', '{}');
    INSERT INTO branches (id, run_id, team) VALUES ('b1', 'r1', '{}');
    INSERT INTO events (branch_id, seq, kind, payload) VALUES ('b1', 1, 'message', '{}');
    INSERT INTO suites (id, name, team, tasks, repeats, runner, benchmark)
      VALUES ('s1', 's', '{}', '[]', 1, 'chain', 'b');
    INSERT INTO suite_runs (suite_id, ord, task_id, repeat) VALUES ('s1', 0, 't', 0);
    INSERT INTO experiments (id, name, task_id, benchmark, runs) VALUES ('e1', 'e', 't', 'b', 1);
    INSERT INTO reviews (key, kind, status, model) VALUES ('run:r1', 'run', 'done', 'm');
    INSERT INTO connections (id, name, command) VALUES ('c1', 'c', '[]');";

/// Today's shaked tables with a row each, migrated as shaked does.
async fn todays_shaked_migrated(c: &mut tokio_postgres::Client) {
    c.batch_execute(include_str!("fixtures/shaked-bb8e7e8.sql"))
        .await
        .unwrap();
    c.batch_execute(TODAYS_ROWS).await.unwrap();
    migrate(c, &LOCAL, &all()).await.unwrap();
}

fn latest(s: Stream) -> u32 {
    migrations()
        .iter()
        .filter(|m| m.stream == s)
        .map(|m| m.version)
        .max()
        .unwrap_or(0)
}

#[tokio::test]
async fn empty_to_latest_then_again_is_a_no_op() {
    let db = Scratch::new("noop").await;
    let mut c = db.connect().await;
    let first = migrate(&mut c, &LOCAL, &all()).await.unwrap();
    assert_eq!(first.applied.len(), migrations().len());
    let again = migrate(&mut c, &LOCAL, &all()).await.unwrap();
    assert!(again.applied.is_empty(), "{:?}", again.applied);
    let rows = count(&c, "SELECT * FROM schema_meta.migrations").await;
    assert_eq!(rows as usize, migrations().len());
    assert!(check(&mut c, &LOCAL, &all()).await.is_ok());
}

/// The embedded set, with `(stream, version)` also accepting `old`.
fn also_accepting(stream: Stream, version: u32, old: [u8; 32]) -> Vec<Migration> {
    let accepted: &'static [[u8; 32]] = Box::leak(Box::new([old]));
    let mut set = all();
    for m in set
        .iter_mut()
        .filter(|m| (m.stream, m.version) == (stream, version))
    {
        m.also_accept = accepted;
    }
    set
}

#[tokio::test]
async fn a_changed_checksum_refuses_and_also_accept_admits() {
    let db = Scratch::new("checksum").await;
    let mut c = db.connect().await;
    migrate(&mut c, &LOCAL, &all()).await.unwrap();
    let old = [7u8; 32];
    let sql =
        "UPDATE schema_meta.migrations SET checksum = $1 WHERE stream = 'shakedown' AND version = 1";
    c.execute(sql, &[&old.as_slice()]).await.unwrap();
    let refused = migrate(&mut c, &LOCAL, &all()).await;
    assert!(
        matches!(
            refused,
            Err(Error::Checksum {
                stream: SHAKEDOWN,
                version: 1,
                ..
            })
        ),
        "{refused:?}"
    );
    let checked = check(&mut c, &LOCAL, &all()).await;
    assert!(
        matches!(checked, Err(Error::Checksum { .. })),
        "{checked:?}"
    );
    let set = also_accepting(SHAKEDOWN, 1, old);
    let r = migrate(&mut c, &LOCAL, &set).await.unwrap();
    assert!(r.applied.is_empty());
}

const EXTRA_STREAM: Stream = Stream("extra");

/// Applies the base and needs stream `extra` 1, which another service applies.
const NEEDS_EXTRA: Plan<'static> = Plan {
    apply: &[SHAKEDOWN],
    require: &[(EXTRA_STREAM, 1)],
    applied_by: "test",
};

fn is_behind(r: &Result<shakedown_schema::Report, Error>, s: Stream, h: u32, n: u32) -> bool {
    matches!(r, Err(Error::Behind { stream, have, need }) if (*stream, *have, *need) == (s, h, n))
}

#[tokio::test]
async fn a_missing_required_stream_reports_behind_and_wait_ready_gives_up() {
    let db = Scratch::new("behind").await;
    let mut c = db.connect().await;
    let r = migrate(&mut c, &NEEDS_EXTRA, &all()).await;
    assert!(is_behind(&r, EXTRA_STREAM, 0, 1), "{r:?}");
    let shakedown = "SELECT * FROM schema_meta.migrations WHERE stream = 'shakedown'";
    assert_eq!(count(&c, shakedown).await, 0);
    let started = std::time::Instant::now();
    let (every, give_up) = (Duration::from_millis(10), Duration::from_millis(50));
    let r = wait_ready(&mut c, &NEEDS_EXTRA, &all(), every, give_up).await;
    assert!(is_behind(&r, EXTRA_STREAM, 0, 1), "{r:?}");
    assert!(started.elapsed() >= give_up);
}

#[tokio::test]
async fn two_concurrent_migrators_apply_each_version_once() {
    let db = Scratch::new("concurrent").await;
    let (mut a, mut b) = (db.connect().await, db.connect().await);
    let set = all();
    let (ra, rb) = tokio::join!(migrate(&mut a, &LOCAL, &set), migrate(&mut b, &LOCAL, &set));
    let (ra, rb) = (ra.unwrap(), rb.unwrap());
    assert_eq!(ra.applied.len() + rb.applied.len(), migrations().len());
    let rows = count(&a, "SELECT * FROM schema_meta.migrations").await;
    assert_eq!(rows as usize, migrations().len());
}

#[tokio::test]
async fn newer_versions_than_the_binary_knows_are_reported_not_refused() {
    let db = Scratch::new("newer").await;
    let mut c = db.connect().await;
    migrate(&mut c, &LOCAL, &all()).await.unwrap();
    let v = latest(SHAKEDOWN) as i32 + 1;
    c.execute(
        "INSERT INTO schema_meta.migrations (stream, version, name, checksum, applied_by)
         VALUES ('shakedown', $1, 'from_the_future', '\\x00', 'shaked')",
        &[&v],
    )
    .await
    .unwrap();
    let r = migrate(&mut c, &LOCAL, &all()).await.unwrap();
    assert!(r.applied.is_empty());
    assert_eq!(r.newer, vec![(SHAKEDOWN, v as u32)]);
}

#[tokio::test]
async fn shakedown_1_is_idempotent_over_tables_made_by_todays_shaked() {
    let old = Scratch::new("todaysshaked").await;
    let fresh = Scratch::new("shakedfresh").await;
    let mut a = old.connect().await;
    todays_shaked_migrated(&mut a).await;
    assert_eq!(
        count(&a, "SELECT * FROM shakedown.events WHERE branch_id = 'b1'").await,
        1
    );
    assert_eq!(
        count(&a, "SELECT * FROM pg_tables WHERE schemaname = 'public'").await,
        0
    );
    let mut b = fresh.connect().await;
    migrate(&mut b, &LOCAL, &all()).await.unwrap();
    let fp = fingerprint(&b, "shakedown").await;
    assert!(
        fp.len() > 60,
        "the fingerprint sees the tables: {}",
        fp.len()
    );
    assert_eq!(fingerprint(&a, "shakedown").await, fp);
}

const EXTRA: Files = Files {
    stream: Stream("extra"),
    files: &[(
        "0001_extra.sql",
        include_str!("fixtures/extra/0001_extra.sql"),
    )],
    also_accept: &[],
};
const WITH_EXTRA: Plan<'static> = Plan {
    apply: &[SHAKEDOWN, Stream("extra")],
    require: &[],
    applied_by: "test",
};

/// The base and stream `extra` applied.
async fn with_extra(c: &mut tokio_postgres::Client) {
    let set = load(&[&BASE_FILES, &EXTRA]).unwrap();
    migrate(c, &WITH_EXTRA, &set).await.unwrap();
}

#[tokio::test]
async fn a_stream_the_crate_does_not_know_applies_after_the_base() {
    let db = Scratch::new("extra").await;
    let mut c = db.connect().await;
    with_extra(&mut c).await;
    let extra = "SELECT * FROM schema_meta.migrations WHERE stream = 'extra' AND version = 1";
    assert_eq!(count(&c, extra).await, 1);
    assert!(support::exists(&c, "extra.t").await);
}

#[tokio::test]
async fn a_binary_that_knows_only_the_base_reports_the_other_stream_as_newer_not_an_error() {
    let db = Scratch::new("extranewer").await;
    let mut c = db.connect().await;
    with_extra(&mut c).await;
    let base_only = Plan {
        apply: &[SHAKEDOWN],
        require: &[],
        applied_by: "test",
    };
    let r = migrate(&mut c, &base_only, base()).await.unwrap();
    assert!(r.applied.is_empty(), "{:?}", r.applied);
    assert_eq!(r.newer, vec![(Stream("extra"), 1)]);
}
