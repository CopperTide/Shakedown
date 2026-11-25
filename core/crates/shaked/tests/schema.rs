//! shaked's records in the shakedown schema through shakedown-schema: old databases move in place,
//! a fresh one has only Shakedown's schemas, a plain login opens its own database, and a database
//! from the retired factory build is refused with the way to convert it.

use serde_json::json;
use shaked::{model::*, pg::Admin, store::Store};
use support::{legacy, pg, run};

mod support;
use support::records;
use support::team;

/// The rows of `sql` in database `db`, each as one string.
async fn rows(db: &str, sql: &str) -> Vec<String> {
    let (c, conn) = tokio_postgres::connect(&pg::admin().db_url(db), tokio_postgres::NoTls)
        .await
        .unwrap();
    let task = tokio::spawn(conn);
    let out = c
        .query(&format!("SELECT ({sql})::text"), &[])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    drop(c);
    let _ = task.await;
    out
}

#[test]
fn an_old_public_database_moves_to_shakedown_and_keeps_its_runs() {
    run(async {
        let clean = pg::Cleanup::new();
        let name = pg::db_name("oldpublic");
        clean.add(&name);
        let (run_id, branch) = legacy::old_public_db(&name).await;
        let store = Store::open_named(&pg::admin(), &name).await.unwrap();
        let runs = records(&store).runs().await.unwrap();
        assert_eq!(
            runs.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            [run_id.as_str()]
        );
        let events = records(&store).events(&branch).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].payload, json!({"text": "kept"}));
        drop(store);
        let history =
            "SELECT string_agg(stream || version || applied_at, ',') FROM schema_meta.migrations";
        let before = rows(&name, history).await;
        let tables = "SELECT count(*) FROM pg_tables WHERE schemaname = 'public'";
        assert_eq!(rows(&name, tables).await, ["0"]);
        drop(Store::open_named(&pg::admin(), &name).await.unwrap());
        assert_eq!(
            rows(&name, history).await,
            before,
            "a second start changes nothing"
        );
    })
}

#[test]
fn a_fresh_database_has_only_shakedown_s_schemas() {
    run(async {
        let (name, store, _clean) = pg::fresh_store("onlyshakedown").await;
        drop(store);
        let schemas = "SELECT string_agg(nspname, ',' ORDER BY nspname) FROM pg_namespace
                        WHERE nspname NOT LIKE 'pg\\_%' AND nspname <> 'information_schema'";
        assert_eq!(rows(&name, schemas).await, ["public,schema_meta,shakedown"]);
        let history = "SELECT string_agg(stream || ' ' || version, ',' ORDER BY stream, version)
                         FROM schema_meta.migrations";
        assert_eq!(
            rows(&name, history).await,
            ["shakedown 1,shakedown 2,shakedown 3,shakedown 4,shakedown 5,shakedown 6"]
        );
    })
}

#[test]
fn running_branches_are_interrupted_at_start() {
    run(async {
        let (_name, store, _clean) = pg::fresh_store("interruptjob").await;
        let run_id = records(&store).create_run("task", &team()).await.unwrap();
        let b = records(&store)
            .create_branch(&run_id, None, &team(), &json!([]), None)
            .await
            .unwrap();
        shaked::jobs::interrupt_at_start(&store).await.unwrap();
        let got = records(&store).branch(&b).await.unwrap();
        assert_eq!(got.status, BranchStatus::Interrupted);
        assert!(got.ended_at.is_some());
    })
}

#[test]
fn a_plain_login_that_owns_its_database_opens_it() {
    run(async {
        let login = pg::TempLogin::new("plainlogin").await;
        let store = Store::open_named(&Admin::lazy(&login.url), &login.db)
            .await
            .expect("a login that owns its database needs nothing else");
        let run_id = records(&store).create_run("task", &team()).await.unwrap();
        let runs = records(&store).runs().await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, run_id);
    })
}

/// The retired factory build's files, in the order it applied them.
const RETIRED: &[(&str, u32, &str, &str)] = &[
    (
        "identity",
        1,
        "baseline",
        include_str!("fixtures/retired/identity/0001_baseline.sql"),
    ),
    (
        "identity",
        2,
        "orgs",
        include_str!("fixtures/retired/identity/0002_orgs.sql"),
    ),
    (
        "shakedown",
        1,
        "baseline",
        include_str!("../../shakedown-schema/migrations/shakedown/0001_baseline.sql"),
    ),
    (
        "shakedown",
        2,
        "orgs",
        include_str!("fixtures/retired/shakedown/0002_orgs.sql"),
    ),
    (
        "shakedown",
        3,
        "validate",
        include_str!("fixtures/retired/shakedown/0003_validate.sql"),
    ),
];

/// The local org the retired build seeded between its identity and shakedown streams.
const RETIRED_SEED: &str = "INSERT INTO identity.orgs (id, slug, name, kind)
    VALUES ('00000000-0000-0000-0000-00000000000b', 'local', 'Local', 'local')";

/// Database `db` as the retired factory build left it: its files applied and recorded with
/// their sha256, as its migrator did.
async fn retired_build(db: &str) {
    let mut sql = String::from(include_str!("fixtures/retired/bootstrap.sql"));
    sql.push_str(
        "\nCREATE SCHEMA IF NOT EXISTS schema_meta;
         CREATE TABLE schema_meta.migrations (stream text NOT NULL, version integer NOT NULL,
           name text NOT NULL, checksum bytea NOT NULL, applied_at timestamptz NOT NULL DEFAULT now(),
           applied_by text NOT NULL, execution_ms integer NOT NULL DEFAULT 0, PRIMARY KEY (stream, version));\n",
    );
    for (stream, version, name, text) in RETIRED {
        if (*stream, *version) == ("shakedown", 2) {
            sql.push_str(RETIRED_SEED);
            sql.push_str(";\n");
        }
        let sum = ring::digest::digest(&ring::digest::SHA256, text.as_bytes());
        let hex: String = sum.as_ref().iter().map(|b| format!("{b:02x}")).collect();
        sql.push_str(&format!(
            "BEGIN;\n{text}\n;\nRESET search_path;\nINSERT INTO schema_meta.migrations (stream, version, name, checksum, applied_by)
             VALUES ('{stream}', {version}, '{name}', decode('{hex}', 'hex'), 'shaked factory');\nCOMMIT;\n"
        ));
    }
    pg::execute(db, &sql).await;
}

#[test]
fn a_database_from_the_retired_factory_build_is_refused_naming_the_fix() {
    run(async {
        let clean = pg::Cleanup::new();
        let name = pg::db_name("retired");
        clean.add(&name);
        pg::admin()
            .create_db_from(&name, "template1")
            .await
            .unwrap();
        retired_build(&name).await;
        let count = "SELECT count(*) FROM schema_meta.migrations";
        assert_eq!(rows(&name, count).await, ["5"]);
        let err = Store::open_named(&pg::admin(), &name)
            .await
            .err()
            .expect("a database the retired build migrated is refused");
        let msg = format!("{err:#}");
        assert!(msg.contains("unreleased factory build"), "{msg}");
        assert!(msg.contains("step O1"), "{msg}");
        assert!(
            msg.contains("core/scripts/o1-retire-factory-orgs.sql"),
            "{msg}"
        );
        assert!(!msg.contains("postgres://"), "no connection string: {msg}");
        assert_eq!(rows(&name, count).await, ["5"], "nothing was written");
    })
}
