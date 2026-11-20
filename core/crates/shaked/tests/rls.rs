//! Row-level security, the second wall: under `shaked_app`, no transaction reads or writes another
//! org's rows on any table, root resources are visible by role, seat, authorship or grant, viewers
//! and billing cannot write, and no scope means no rows.

use shaked::{
    auth::{test_scope, Principal},
    store::{Store, Tx},
};
use support::{orgs, pg, run};
use uuid::Uuid;

mod support;

/// Every table in schema shakedown, in an order their fixture rows can be inserted in.
const SHAKEDOWN_TABLES: &[&str] = &[
    "runs",
    "branches",
    "events",
    "experiments",
    "suites",
    "suite_runs",
    "reviews",
    "connections",
    "provider_keys",
    "worlds",
    "org_settings",
    "saves",
    "sandboxes",
    "api_tokens",
    "run_tokens",
    "grants",
    "quotas",
    "usage",
    "usage_monthly",
];

/// Each table's fixture row: its columns besides org_id and their values, where `N` makes ids and
/// unique values the row's own, `ORG` is its org and `USER` a member of it.
const ROWS: &[(&str, &str, &str)] = &[
    ("runs", "id, task_id, team", "'rN', 't', '{}'"),
    ("branches", "id, run_id, team", "'bN', 'r1', '{}'"),
    (
        "events",
        "branch_id, seq, kind, payload",
        "'b1', 1N, 'message', '{}'",
    ),
    (
        "experiments",
        "id, name, task_id, benchmark, runs",
        "'eN', 'e', 't', 'b', 1",
    ),
    (
        "suites",
        "id, name, team, tasks, repeats, runner, benchmark",
        "'sN', 's', '{}', '[]', 1, 'chain', 'b'",
    ),
    (
        "suite_runs",
        "suite_id, ord, task_id, repeat",
        "'s1', 1N, 't', 0",
    ),
    (
        "reviews",
        "key, kind, status, model",
        "'run:rN', 'run', 'done', 'm'",
    ),
    ("connections", "id, name, command", "'cN', 'c', '[]'"),
    (
        "provider_keys",
        "name, kind, sealed_key, key_last4",
        "'kN', 'anthropic', '\\x00', 'abcd'",
    ),
    (
        "worlds",
        "name, package, version, store",
        "'wN', 'p', '1', 'default'",
    ),
    ("org_settings", "retention_days", "30"),
    (
        "saves",
        "run_id, branch_id, integration, save_id, seq, store, object_key",
        "'r1', 'b1', 'i', 'sN', 1, 'default', 'ORG/saves/i/sN/'",
    ),
    (
        "sandboxes",
        "run_id, branch_id, backend, image_digest, state",
        "'r1', 'b1', 'fake', 'dN', 'deleted'",
    ),
    (
        "api_tokens",
        "user_id, name, token_hash, token_prefix, scopes",
        "'USER', 't', 'hashN', 'sd_00000', '{read}'",
    ),
    (
        "run_tokens",
        "run_id, branch_id, kind, token_hash, expires_at",
        "'r1', 'b1', 'run', 'rhashN', now() + interval '1 hour'",
    ),
    (
        "grants",
        "grantee_user_id, level, run_id",
        "'USER', 'view', 'r1'",
    ),
    (
        "quotas",
        "meter, period, limit_value",
        "'runs', 'month', 10",
    ),
    (
        "usage",
        "meter, quantity, source, idempotency_key",
        "'runs', 1, 'api', 'keyN'",
    ),
    (
        "usage_monthly",
        "month, meter, quantity",
        "date '2026-11-01', 'runsN', 1",
    ),
];

/// The fixture row of `table` in `org`, numbered `n`.
fn row_sql(table: &str, org: Uuid, user: Uuid, n: &str) -> String {
    let (_, cols, vals) = ROWS.iter().find(|r| r.0 == table).expect("a fixture row");
    let vals = vals
        .replace('N', n)
        .replace("ORG", &org.to_string())
        .replace("USER", &user.to_string());
    format!("INSERT INTO shakedown.{table} (org_id, {cols}) VALUES ('{org}', {vals})")
}

/// A store on a fresh database with the org fixtures; dropped with the guard.
async fn store_with_orgs(tag: &str) -> (String, Store, orgs::Orgs, pg::Cleanup) {
    let (name, store, clean) = pg::fresh_store(tag).await;
    let orgs = orgs::seed(&name).await;
    (name, store, orgs, clean)
}

fn owner_of(orgs: &orgs::Orgs, org: &str) -> shaked::auth::Scope {
    test_scope(orgs.principal(&format!("{org}_owner")))
}

/// The rows of `table` this transaction sees.
async fn count(tx: &Tx, table: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM shakedown.{table}");
    tx.query_one(&sql, &[]).await.unwrap().get(0)
}

/// Rows `sql` changed, 0 when the role may not change the table at all.
async fn changed(store: &Store, scope: &shaked::auth::Scope, sql: &str) -> u64 {
    let tx = store.tx(scope).await.unwrap();
    match tx.execute(sql, &[]).await {
        Ok(n) => n,
        Err(e) if format!("{e:#}").contains("permission denied") => 0,
        Err(e) => panic!("{sql}: {e:#}"),
    }
}

#[test]
fn every_shakedown_table_has_forced_rls() {
    run(async {
        let (name, _store, _clean) = pg::fresh_store("rlsforced").await;
        let sql = "SELECT string_agg(relname, ',' ORDER BY relname) FROM pg_class
                    WHERE relnamespace = 'shakedown'::regnamespace AND relkind IN ('r','p')
                      AND NOT (relrowsecurity AND relforcerowsecurity)";
        assert_eq!(pg::query_text(&name, sql).await, None);
        let all = "SELECT string_agg(relname, ',' ORDER BY relname) FROM pg_class
                    WHERE relnamespace = 'shakedown'::regnamespace AND relkind IN ('r','p')";
        let mut listed = SHAKEDOWN_TABLES.to_vec();
        listed.sort();
        assert_eq!(
            pg::query_text(&name, all).await,
            Some(listed.join(",")),
            "the list names every table"
        );
    })
}

#[test]
fn every_identity_org_table_has_rls() {
    run(async {
        let (name, _store, _clean) = pg::fresh_store("rlsidentity").await;
        let sql = "SELECT string_agg(relname, ',' ORDER BY relname) FROM pg_class
                    WHERE relnamespace = 'identity'::regnamespace AND relrowsecurity";
        let want = "audit_log,memberships,orgs,service_accounts,team_members,teams,users";
        assert_eq!(pg::query_text(&name, sql).await.as_deref(), Some(want));
    })
}

#[test]
fn rls_refuses_cross_org_reads_and_writes_on_every_table() {
    run(async {
        let (name, store, orgs, _clean) = store_with_orgs("rlscross").await;
        let (a, a_member) = (orgs.org("a").id, orgs.user("a_member").id);
        let b = owner_of(&orgs, "b");
        for t in SHAKEDOWN_TABLES {
            pg::execute(&name, &row_sql(t, a, a_member, "1")).await;
            let tx = store.tx(&b).await.unwrap();
            assert_eq!(count(&tx, t).await, 0, "{t} read");
            drop(tx);
            let update = format!("UPDATE shakedown.{t} SET org_id = org_id");
            assert_eq!(changed(&store, &b, &update).await, 0, "{t} update");
            assert_eq!(
                changed(&store, &b, &format!("DELETE FROM shakedown.{t}")).await,
                0,
                "{t} delete"
            );
            let tx = store.tx(&b).await.unwrap();
            let insert = tx.execute(&row_sql(t, a, a_member, "2"), &[]).await;
            assert!(insert.is_err(), "{t} insert into A");
        }
        let a_owner = owner_of(&orgs, "a");
        let tx = store.tx(&a_owner).await.unwrap();
        for t in SHAKEDOWN_TABLES {
            assert!(count(&tx, t).await >= 1, "{t}: A's owner sees A's row");
        }
    })
}

/// A run of org A scoped to its team, made by its admin.
async fn team_run(name: &str, orgs: &orgs::Orgs) {
    let (a, team, admin) = (
        orgs.org("a").id,
        orgs.org("a").team.id,
        orgs.user("a_admin").id,
    );
    let sql = format!(
        "INSERT INTO shakedown.runs (id, org_id, team_id, created_by, task_id, team)
         VALUES ('teamrun', '{a}', '{team}', '{admin}', 't', '{{}}')"
    );
    pg::execute(name, &sql).await;
}

async fn sees_team_run(store: &Store, p: Principal) -> bool {
    let tx = store.tx(&test_scope(p)).await.unwrap();
    let n: i64 = (tx
        .query_one("SELECT count(*) FROM runs WHERE id = 'teamrun'", &[])
        .await)
        .unwrap()
        .get(0);
    n == 1
}

#[test]
fn root_visibility_by_team_seat_creator_and_grant() {
    run(async {
        let (name, store, orgs, _clean) = store_with_orgs("rlsroot").await;
        team_run(&name, &orgs).await;
        assert!(
            !sees_team_run(&store, orgs.principal("a_member")).await,
            "no seat"
        );
        assert!(
            sees_team_run(&store, orgs.principal("a_seated")).await,
            "a seat"
        );
        assert!(
            sees_team_run(&store, orgs.principal("a_owner")).await,
            "an owner"
        );
        let member = orgs.user("a_member").id;
        let own = format!("UPDATE shakedown.runs SET created_by = '{member}' WHERE id = 'teamrun'");
        pg::execute(&name, &own).await;
        assert!(
            sees_team_run(&store, orgs.principal("a_member")).await,
            "its creator"
        );
    })
}

#[test]
fn a_grant_shows_a_team_run() {
    run(async {
        let (name, store, orgs, _clean) = store_with_orgs("rlsgrant").await;
        team_run(&name, &orgs).await;
        let (a, viewer) = (orgs.org("a").id, orgs.user("a_viewer").id);
        assert!(!sees_team_run(&store, orgs.principal("a_viewer")).await);
        let grant = format!(
            "INSERT INTO shakedown.grants (org_id, grantee_user_id, level, run_id)
             VALUES ('{a}', '{viewer}', 'view', 'teamrun')"
        );
        pg::execute(&name, &grant).await;
        assert!(
            sees_team_run(&store, orgs.principal("a_viewer")).await,
            "a grant"
        );
    })
}

#[test]
fn viewers_and_billing_cannot_write() {
    run(async {
        let (name, store, orgs, _clean) = store_with_orgs("rlswrite").await;
        let (a, member) = (orgs.org("a").id, orgs.user("a_member").id);
        let viewer = test_scope(orgs.principal("a_viewer"));
        let tx = store.tx(&viewer).await.unwrap();
        assert!(
            tx.execute(&row_sql("runs", a, member, "1"), &[])
                .await
                .is_err(),
            "viewer insert"
        );
        pg::execute(&name, &row_sql("runs", a, member, "1")).await;
        let billing = test_scope(orgs.principal("a_billing"));
        let tx = store.tx(&billing).await.unwrap();
        assert_eq!(count(&tx, "runs").await, 0, "billing reads no runs");
    })
}

#[test]
fn no_scope_no_rows() {
    run(async {
        let (name, _store, orgs, _clean) = store_with_orgs("rlsnoscope").await;
        let (a, member) = (orgs.org("a").id, orgs.user("a_member").id);
        for t in SHAKEDOWN_TABLES {
            pg::execute(&name, &row_sql(t, a, member, "1")).await;
        }
        for t in SHAKEDOWN_TABLES {
            let sql = format!("SET ROLE shaked_app; SELECT count(*)::text FROM shakedown.{t}");
            assert_eq!(
                pg::batch_last_text(&name, &sql).await.as_deref(),
                Some("0"),
                "{t}"
            );
        }
    })
}

#[test]
fn an_uncommitted_tx_never_returns_its_connection() {
    run(async {
        let (_name, store, orgs, _clean) = store_with_orgs("rlsdrop").await;
        let tx = store.tx(&owner_of(&orgs, "a")).await.unwrap();
        let before = store.pool_status().size;
        drop(tx);
        assert_eq!(store.pool_status().size, before - 1);
        let tx = store.tx(&owner_of(&orgs, "a")).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            store.pool_status().size,
            before,
            "a committed one goes back"
        );
    })
}

/// A run of org A with a branch and one event, made through the store as A's owner: the run
/// and the branch.
async fn org_a_branch(store: &Store, a: &shaked::auth::Scope) -> (String, String) {
    let team = shaked::model::Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![],
    };
    let at = store.at(a);
    let run = at.create_run("t", &team).await.unwrap();
    let none = serde_json::json!([]);
    let br = at
        .create_branch(&run, None, &team, &none, None)
        .await
        .unwrap();
    let kind = shaked::model::EventKind::Message;
    at.append(&br, None, kind, serde_json::json!({}))
        .await
        .unwrap();
    (run, br)
}

#[test]
fn org_b_cannot_read_or_append_to_org_a_branches_through_the_store() {
    run(async {
        let (_name, store, orgs, _clean) = store_with_orgs("scopedcross").await;
        let (a, b) = (owner_of(&orgs, "a"), owner_of(&orgs, "b"));
        let (run, br) = org_a_branch(&store, &a).await;
        let (in_a, in_b) = (store.at(&a), store.at(&b));
        let kind = shaked::model::EventKind::Message;
        assert!(in_b.find_branch(&br).await.unwrap().is_none());
        assert!(in_b.events(&br).await.unwrap().is_empty());
        assert!(in_b
            .append(&br, None, kind, serde_json::json!({}))
            .await
            .is_err());
        assert!(in_b.runs().await.unwrap().is_empty());
        assert!(!in_b.run_exists(&run).await.unwrap());
        assert_eq!(
            in_a.events(&br).await.unwrap().len(),
            1,
            "B's append changed nothing"
        );
    })
}

#[test]
fn a_branch_of_an_invisible_run_is_not_found() {
    run(async {
        let (name, store, orgs, _clean) = store_with_orgs("scopedinvisible").await;
        team_run(&name, &orgs).await;
        let a = orgs.org("a").id;
        let sql = format!(
            "INSERT INTO shakedown.branches (id, org_id, run_id, team) VALUES ('teambranch', '{a}', 'teamrun', '{{\"id\": \"t\", \"name\": \"T\", \"agents\": []}}')"
        );
        pg::execute(&name, &sql).await;
        let member = test_scope(orgs.principal("a_member"));
        assert!(store
            .at(&member)
            .find_branch("teambranch")
            .await
            .unwrap()
            .is_none());
        let seated = test_scope(orgs.principal("a_seated"));
        assert!(store
            .at(&seated)
            .find_branch("teambranch")
            .await
            .unwrap()
            .is_some());
    })
}

#[test]
fn inserts_without_org_fail_after_shakedown_3() {
    run(async {
        let (name, _store, _clean) = pg::fresh_store("noorgdefault").await;
        let sql = "INSERT INTO shakedown.runs (id, task_id, team) VALUES ('r', 't', '{}')";
        let (c, conn) = tokio_postgres::connect(&pg::admin().db_url(&name), tokio_postgres::NoTls)
            .await
            .unwrap();
        tokio::spawn(conn);
        let err = c
            .batch_execute(sql)
            .await
            .expect_err("org_id has no default any more");
        assert!(format!("{err:?}").contains("org_id"), "{err:?}");
    })
}
