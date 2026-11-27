//! A project's state in Postgres (migration shakedown 10) through the store: settings and items
//! written with a revision check per row, soft deletes, the undo history and git import records.
//! Nothing in the engine reads these tables yet.

use chrono::SubsecRound;
use serde_json::{json, Value};
use shaked::store::{
    Base, ChangeItem, ChangeKind, ImportRecord, ItemKind, ItemWrite, ProjectItem, ProjectRow,
    Scoped, SlugTaken, Stale,
};
use shakedown_project::format::{AssistantSettings, ProjectFile, SpendPolicy};
use support::{pg, records, run};
use uuid::Uuid;

mod support;

fn settings(name: &str) -> ProjectFile {
    let yaml = format!("format: shakedown-project/1\nname: {name}");
    serde_yaml_ng::from_str(&yaml).unwrap()
}

fn scenario(label: &str) -> Value {
    json!({"format": "shakedown-scenario/1", "label": label, "world": "mill", "task": "t"})
}

async fn project(s: Scoped<'_>, name: &str) -> ProjectRow {
    s.create_project(&settings(name)).await.unwrap()
}

async fn put(
    s: Scoped<'_>,
    p: Uuid,
    slug: &str,
    base: Option<Base>,
) -> anyhow::Result<ProjectItem> {
    let body = scenario(slug);
    let w = ItemWrite {
        kind: ItemKind::Scenario,
        slug,
        body: &body,
        base,
    };
    s.put_project_item(p, w).await
}

fn base(i: &ProjectItem) -> Base {
    Base {
        id: i.id,
        revision: i.revision,
    }
}

fn stale(e: &anyhow::Error) -> &Stale {
    e.downcast_ref::<Stale>().unwrap_or_else(|| panic!("{e:#}"))
}

#[test]
fn settings_round_trip_and_a_stale_update_is_refused() {
    run(async {
        let (_name, store, _clean) = pg::fresh_store("projsettings").await;
        let s = records(&store);
        let made = project(s, "Mill").await;
        let mut want = settings("Mill");
        want.defaults.world = Some("mill".into());
        want.defaults.cost_cap_usd = Some(2.5);
        want.permissions.spend = SpendPolicy::UnderCap { usd_per_day: 10.0 };
        want.assistant = Some(AssistantSettings {
            model: "claude-sonnet".into(),
            provider: Some("anthropic".into()),
        });
        let first = Base {
            id: made.id,
            revision: 1,
        };
        let updated = s.update_project(first, &want).await.unwrap();
        assert_eq!((updated.revision, &updated.settings), (2, &want));
        let read = s.project_items(made.id).await.unwrap().unwrap();
        assert_eq!(read.project, updated);
        let e = s.update_project(first, &settings("x")).await.unwrap_err();
        assert_eq!(stale(&e).current, Some(2), "{e}");
        assert!(s.project_items(Uuid::new_v4()).await.unwrap().is_none());
    })
}

#[test]
fn items_are_written_with_a_revision_check_and_slugs_are_unique_per_project() {
    run(async {
        let (_name, store, _clean) = pg::fresh_store("projitems").await;
        let s = records(&store);
        let (p, other) = (project(s, "A").await.id, project(s, "B").await.id);
        let made = put(s, p, "rush", None).await.unwrap();
        assert_eq!((made.revision, &made.body), (1, &scenario("rush")));
        let edited = put(s, p, "rush", Some(base(&made))).await.unwrap();
        assert_eq!((edited.id, edited.revision), (made.id, 2));
        let e = put(s, p, "rush", Some(base(&made))).await.unwrap_err();
        assert_eq!((stale(&e).based_on, stale(&e).current), (1, Some(2)));
        let e = put(s, p, "rush", None).await.unwrap_err();
        assert!(e.downcast_ref::<SlugTaken>().is_some(), "{e:#}");
        put(s, other, "rush", None).await.unwrap();
        let renamed = put(s, p, "surge", Some(base(&edited))).await.unwrap();
        assert_eq!((renamed.slug.as_str(), renamed.revision), ("surge", 3));
        put(s, p, "calm", None).await.unwrap();
        let items = s.project_items(p).await.unwrap().unwrap().items;
        let slugs: Vec<_> = items.iter().map(|i| i.slug.as_str()).collect();
        assert_eq!(
            slugs,
            ["calm", "surge"],
            "by slug, the other project's left out"
        );
    })
}

/// The rows of `sql` in database `db`, counted.
async fn count(db: &str, sql: &str) -> i64 {
    let url = pg::admin().db_url(db);
    let (c, conn) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
        .await
        .unwrap();
    let task = tokio::spawn(conn);
    let n = c.query_one(sql, &[]).await.unwrap().get(0);
    drop(c);
    let _ = task.await;
    n
}

#[test]
fn a_delete_marks_the_row_and_frees_its_slug() {
    run(async {
        let (name, store, _clean) = pg::fresh_store("projdelete").await;
        let s = records(&store);
        let p = project(s, "A").await.id;
        let made = put(s, p, "rush", None).await.unwrap();
        let gone = s
            .delete_project_item(p, ItemKind::Scenario, base(&made))
            .await
            .unwrap();
        assert_eq!(gone.revision, 2);
        let e = s
            .delete_project_item(p, ItemKind::Scenario, base(&made))
            .await;
        assert_eq!(
            stale(&e.unwrap_err()).current,
            None,
            "deleted: no live revision"
        );
        let e = put(s, p, "rush", Some(base(&gone))).await.unwrap_err();
        assert_eq!(stale(&e).current, None, "a deleted item takes no write");
        assert!(s.project_items(p).await.unwrap().unwrap().items.is_empty());
        put(s, p, "rush", None).await.unwrap();
        let rows = "SELECT count(*) FROM shakedown.project_scenarios WHERE slug = 'rush'";
        assert_eq!(count(&name, rows).await, 2, "the deleted row is kept");
    })
}

fn change_items(setup: Uuid) -> Vec<ChangeItem> {
    let before = json!({"format": "shakedown-setup/1", "label": "Old"});
    vec![
        ChangeItem::row(ChangeKind::Setup, setup, "duo", Some(before), None).at_revision(4),
        ChangeItem::file(
            "worlds/mill/data/orders.csv",
            Some(b"id\n1\n".to_vec()),
            None,
        ),
    ]
}

#[test]
fn changes_keep_their_items_in_order_newest_first() {
    run(async {
        let (_name, store, _clean) = pg::fresh_store("projchanges").await;
        let s = records(&store);
        let p = project(s, "A").await.id;
        let setup = Uuid::new_v4();
        let first = s
            .record_project_change(p, "delete setup duo", &change_items(setup))
            .await
            .unwrap();
        let undo = ChangeItem::row(
            ChangeKind::Project,
            p,
            "a",
            None,
            Some(json!({"name": "A"})),
        );
        let second = s
            .record_project_change(p, "edit project", &[undo])
            .await
            .unwrap();
        let all = s.project_changes(p, 10).await.unwrap();
        let ids: Vec<_> = all.iter().map(|c| c.id).collect();
        assert_eq!(ids, [second, first]);
        assert_eq!(all[1].label, "delete setup duo");
        assert_eq!(all[1].items, change_items(setup));
        assert!(all[1].undone_at.is_none());
        assert_eq!(s.project_changes(p, 1).await.unwrap().len(), 1);
    })
}

fn import(commit: &str) -> ImportRecord {
    ImportRecord {
        slug: "crew".into(),
        url: "https://example.test/crew.git".into(),
        git_ref: "HEAD".into(),
        commit_sha: commit.into(),
        imported_at: chrono::Utc::now().trunc_subsecs(6),
        packages: json!([{"name": "crew", "kind": "kit", "dir": "."}]),
        items: json!([{"kind": "setup", "id": "crew", "digest": "abc"}]),
        edits: json!([]),
    }
}

#[test]
fn a_re_import_replaces_its_record() {
    run(async {
        let (_name, store, _clean) = pg::fresh_store("projimports").await;
        let s = records(&store);
        let p = project(s, "A").await.id;
        s.upsert_project_import(p, &import("aaaa")).await.unwrap();
        let again = import("bbbb");
        s.upsert_project_import(p, &again).await.unwrap();
        let other = ImportRecord {
            slug: "tools".into(),
            ..import("cccc")
        };
        s.upsert_project_import(p, &other).await.unwrap();
        assert_eq!(s.project_imports(p).await.unwrap(), [again, other]);
    })
}
