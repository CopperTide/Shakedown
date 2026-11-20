//! The migrator against a real Postgres (the compose server, a scratch database per test).

mod support;

use std::time::Duration;

use coppertide_schema::{
    check, migrate, migrate_set, migrations, wait_ready, Error, Migration, Plan, Stream,
};
use support::{count, fingerprint, Scratch};

const LOCAL: Plan<'static> = Plan {
    apply: &[Stream::Identity, Stream::Shakedown],
    require: &[],
    applied_by: "test",
};
const IDENTITY: Plan<'static> = Plan {
    apply: &[Stream::Identity],
    require: &[],
    applied_by: "test",
};

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
    let db = Scratch::bootstrapped("noop").await;
    let mut c = db.connect().await;
    let first = migrate(&mut c, &LOCAL).await.unwrap();
    assert_eq!(first.applied.len(), migrations().len());
    let again = migrate(&mut c, &LOCAL).await.unwrap();
    assert!(again.applied.is_empty(), "{:?}", again.applied);
    let rows = count(&c, "SELECT * FROM schema_meta.migrations").await;
    assert_eq!(rows as usize, migrations().len());
    assert!(check(&mut c, &LOCAL).await.is_ok());
}

/// The embedded set, with `(stream, version)` also accepting `old`.
fn also_accepting(stream: Stream, version: u32, old: [u8; 32]) -> Vec<Migration> {
    let accepted: &'static [[u8; 32]] = Box::leak(Box::new([old]));
    let mut set = migrations().to_vec();
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
    let db = Scratch::bootstrapped("checksum").await;
    let mut c = db.connect().await;
    migrate(&mut c, &LOCAL).await.unwrap();
    let old = [7u8; 32];
    let sql =
        "UPDATE schema_meta.migrations SET checksum = $1 WHERE stream = 'identity' AND version = 1";
    c.execute(sql, &[&old.as_slice()]).await.unwrap();
    let refused = migrate(&mut c, &LOCAL).await;
    assert!(
        matches!(
            refused,
            Err(Error::Checksum {
                stream: Stream::Identity,
                version: 1,
                ..
            })
        ),
        "{refused:?}"
    );
    let checked = check(&mut c, &LOCAL).await;
    assert!(
        matches!(checked, Err(Error::Checksum { .. })),
        "{checked:?}"
    );
    let set = also_accepting(Stream::Identity, 1, old);
    let r = migrate_set(&mut c, &LOCAL, &set).await.unwrap();
    assert!(r.applied.is_empty());
}

const HOSTED: Plan<'static> = Plan {
    apply: &[Stream::Shakedown],
    require: &[(Stream::Identity, 2)],
    applied_by: "test",
};

fn is_behind(r: &Result<coppertide_schema::Report, Error>, s: Stream, h: u32, n: u32) -> bool {
    matches!(r, Err(Error::Behind { stream, have, need }) if (*stream, *have, *need) == (s, h, n))
}

#[tokio::test]
async fn a_missing_required_stream_reports_behind_and_wait_ready_gives_up() {
    let db = Scratch::bootstrapped("behind").await;
    let mut c = db.connect().await;
    migrate(&mut c, &IDENTITY).await.unwrap();
    // As if the other service had applied only identity 1.
    let sql = "DELETE FROM schema_meta.migrations WHERE stream = 'identity' AND version >= 2";
    c.batch_execute(sql).await.unwrap();
    let r = migrate(&mut c, &HOSTED).await;
    assert!(is_behind(&r, Stream::Identity, 1, 2), "{r:?}");
    let shakedown = "SELECT * FROM schema_meta.migrations WHERE stream = 'shakedown'";
    assert_eq!(count(&c, shakedown).await, 0);
    let started = std::time::Instant::now();
    let (every, give_up) = (Duration::from_millis(10), Duration::from_millis(50));
    let r = wait_ready(&mut c, &HOSTED, every, give_up).await;
    assert!(is_behind(&r, Stream::Identity, 1, 2), "{r:?}");
    assert!(started.elapsed() >= give_up);
}

#[tokio::test]
async fn two_concurrent_migrators_apply_each_version_once() {
    let db = Scratch::bootstrapped("concurrent").await;
    let (mut a, mut b) = (db.connect().await, db.connect().await);
    let (ra, rb) = tokio::join!(migrate(&mut a, &LOCAL), migrate(&mut b, &LOCAL));
    let (ra, rb) = (ra.unwrap(), rb.unwrap());
    assert_eq!(ra.applied.len() + rb.applied.len(), migrations().len());
    let rows = count(&a, "SELECT * FROM schema_meta.migrations").await;
    assert_eq!(rows as usize, migrations().len());
}

#[tokio::test]
async fn newer_versions_than_the_binary_knows_are_reported_not_refused() {
    let db = Scratch::bootstrapped("newer").await;
    let mut c = db.connect().await;
    migrate(&mut c, &LOCAL).await.unwrap();
    let v = latest(Stream::Identity) as i32 + 1;
    c.execute(
        "INSERT INTO schema_meta.migrations (stream, version, name, checksum, applied_by)
         VALUES ('identity', $1, 'from_the_future', '\\x00', 'gateway')",
        &[&v],
    )
    .await
    .unwrap();
    let r = migrate(&mut c, &LOCAL).await.unwrap();
    assert!(r.applied.is_empty());
    assert_eq!(r.newer, vec![(Stream::Identity, v as u32)]);
}

#[tokio::test]
async fn identity_1_create_path_matches_cloud_0015_moved_path() {
    let moved = Scratch::bootstrapped("cloudmoved").await;
    let fresh = Scratch::bootstrapped("cloudfresh").await;
    let mut a = moved.connect().await;
    support::apply_cloud_0015(&a).await;
    let ada = "INSERT INTO users (id, name, email)
               VALUES ('00000000-0000-0000-0000-0000000000c1', 'Ada', 'ada@example.test')";
    a.batch_execute(ada).await.unwrap();
    migrate(&mut a, &IDENTITY).await.unwrap();
    let mut b = fresh.connect().await;
    migrate(&mut b, &IDENTITY).await.unwrap();
    let fp = fingerprint(&b, "identity").await;
    assert!(
        fp.len() > 80,
        "the fingerprint sees the tables: {}",
        fp.len()
    );
    assert_eq!(fingerprint(&a, "identity").await, fp);
    assert_eq!(count(&a, "SELECT * FROM identity.users").await, 1);
    assert!(!support::exists(&a, "public.users").await);
    assert!(
        support::exists(&a, "public.devices").await,
        "cloud tables stay for stream cloud"
    );
}

#[tokio::test]
async fn shakedown_1_is_idempotent_over_tables_made_by_todays_shaked() {
    let old = Scratch::bootstrapped("todaysshaked").await;
    let fresh = Scratch::bootstrapped("shakedfresh").await;
    let mut a = old.connect().await;
    let today = include_str!("fixtures/shaked-bb8e7e8.sql");
    a.batch_execute(today).await.unwrap();
    a.batch_execute(
        "INSERT INTO runs (id, task_id, team) VALUES ('r1', 't', '{}');
         INSERT INTO branches (id, run_id, team) VALUES ('b1', 'r1', '{}');
         INSERT INTO events (branch_id, seq, kind, payload) VALUES ('b1', 1, 'note', '{}');",
    )
    .await
    .unwrap();
    migrate(&mut a, &LOCAL).await.unwrap();
    assert_eq!(
        count(&a, "SELECT * FROM shakedown.events WHERE branch_id = 'b1'").await,
        1
    );
    assert_eq!(
        count(&a, "SELECT * FROM pg_tables WHERE schemaname = 'public'").await,
        0
    );
    let mut b = fresh.connect().await;
    migrate(&mut b, &LOCAL).await.unwrap();
    let fp = fingerprint(&b, "shakedown").await;
    assert!(
        fp.len() > 60,
        "the fingerprint sees the tables: {}",
        fp.len()
    );
    assert_eq!(fingerprint(&a, "shakedown").await, fp);
}

#[tokio::test]
async fn a_missing_setting_matches_no_row() {
    let db = Scratch::bootstrapped("nosetting").await;
    let mut c = db.connect().await;
    migrate(&mut c, &IDENTITY).await.unwrap();
    let org = "00000000-0000-0000-0000-0000000000d1";
    c.batch_execute(&format!(
        "INSERT INTO identity.orgs (id, slug, name, kind) VALUES ('{org}', 'acme', 'Acme', 'customer')"
    ))
    .await
    .unwrap();
    for (set, want) in [("", 0i64), (org, 1)] {
        let tx = c.transaction().await.unwrap();
        tx.batch_execute("SET LOCAL ROLE shaked_app").await.unwrap();
        if !set.is_empty() {
            tx.execute("SELECT set_config('app.org_id', $1, true)", &[&set])
                .await
                .unwrap();
        }
        let n: i64 = tx
            .query_one("SELECT count(*) FROM identity.orgs", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(n, want, "with app.org_id {set:?}");
    }
}
