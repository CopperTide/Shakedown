//! shaked's records in the shakedown schema through coppertide-schema: old databases move in place,
//! a fresh one has the built-in local owner, and a login that cannot set up the roles is refused.

use serde_json::json;
use shaked::{
    model::*,
    pg::Admin,
    store::{DbUrls, Store, LOCAL_ORG_ID, LOCAL_USER_ID},
};
use support::{legacy, pg, run};

mod support;
use support::records;

fn team() -> Team {
    Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![AgentSpec::new("a", AgentKind::Open)
            .with_name("A")
            .with_prompt("p")],
    }
}

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
fn a_fresh_database_has_the_local_owner() {
    run(async {
        let (name, store, _clean) = pg::fresh_store("localowner").await;
        drop(store);
        let sql = format!(
            "SELECT m.role || ' ' || o.kind || ' ' || o.plan FROM identity.memberships m
               JOIN identity.orgs o ON o.id = m.org_id
              WHERE m.org_id = '{LOCAL_ORG_ID}' AND m.user_id = '{LOCAL_USER_ID}'"
        );
        assert_eq!(rows(&name, &sql).await, ["owner local local"]);
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
fn a_non_superuser_without_roles_is_refused_naming_bootstrap_sql() {
    run(async {
        let login = pg::TempLogin::new("nosuper").await;
        let err = Store::open_named(&Admin::lazy(&login.url), &login.db)
            .await
            .err()
            .expect("a login outside shaked_app is refused");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("core/crates/coppertide-schema/bootstrap.sql"),
            "{msg}"
        );
        assert!(
            !msg.contains(&login.password),
            "the message never shows the password: {msg}"
        );
    })
}

/// A database with only stream identity (as the gateway leaves it), its guard, and a login in
/// both shaked roles.
async fn gateway_database() -> (String, pg::Cleanup, pg::TempLogin) {
    let clean = pg::Cleanup::new();
    let name = pg::db_name("hostedopen");
    clean.add(&name);
    pg::admin()
        .create_db_from(&name, "template1")
        .await
        .unwrap();
    pg::identity_only(&name).await;
    let roles = ["shaked_app", "shaked_system"];
    let login = pg::TempLogin::member("hostedopen", &name, &roles).await;
    (name, clean, login)
}

#[test]
fn a_hosted_store_migrates_shakedown_and_never_seeds_the_local_owner() {
    run(async {
        let (name, _clean, login) = gateway_database().await;
        let urls = DbUrls {
            app: login.db_url(&name),
            system: Some(login.db_url(&name)),
            migrate: Some(pg::admin().db_url(&name)),
        };
        drop(Store::open_hosted(&urls).await.unwrap());
        let local = format!("SELECT count(*) FROM identity.orgs WHERE id = '{LOCAL_ORG_ID}'");
        assert_eq!(rows(&name, &local).await, ["0"]);
        let shakedown = "SELECT count(*) FROM schema_meta.migrations WHERE stream = 'shakedown'";
        assert_ne!(rows(&name, shakedown).await, ["0"]);
        let missing = DbUrls {
            migrate: None,
            ..urls
        };
        let err = Store::open_hosted(&missing)
            .await
            .err()
            .expect("hosted needs a migrate URL");
        assert!(
            format!("{err:#}").contains("SHAKEDOWN_MIGRATE_PG_URL"),
            "{err:#}"
        );
    })
}
