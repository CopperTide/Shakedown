//! A world's own data: uploaded into its folder, set one value at a time, deleted; the files it
//! marks "Every run starts from this" reach every part of its runs (`world_data` in the part's
//! start config, recorded in `RunStart.world`), and an edit is new data with a new digest.

mod support;

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};
use shaked::model::EventKind;
use shaked::providers::ProviderConfig;
use support::app::silent_provider;
use support::fake_llm::{fake_llm, gpt_oss};
use support::repo::{fixture_repo, uv_found, write};
use support::{poll_until, records, run, AppFx};

const DATA: &str = "/api/project/worlds/box/data";
const WORLD: &str = "format: shakedown-world/3\nlabel: The box\npackage: wd-bench\ndata:\n- file: data/seed.json\n  seeds: box\n  default_state: true\n";

/// The `world_data` each run of the experiment so far gave its parts.
async fn given(fx: &AppFx) -> Vec<Value> {
    let (_, runs) = fx.get("/api/project/experiments/try/runs").await;
    let mut out = vec![];
    for branch in branch_ids(&runs) {
        let events = records(&fx.state.store)
            .events(&branch)
            .await
            .unwrap_or_default();
        let start = events.iter().find(|e| e.kind == EventKind::RunStart);
        if let Some(data) = start.map(|e| e.payload["world"][0]["config"]["world_data"].clone()) {
            out.push(data);
        }
    }
    out
}

fn branch_ids(v: &Value) -> Vec<String> {
    match v {
        Value::Object(m) => m
            .iter()
            .flat_map(|(k, x)| match (k.as_str(), x) {
                ("branch_id" | "branch", Value::String(s)) => vec![s.clone()],
                _ => branch_ids(x),
            })
            .collect(),
        Value::Array(a) => a.iter().flat_map(branch_ids).collect(),
        _ => vec![],
    }
}

/// Deploys the experiment and waits for a run whose data is not one of `seen`: its data.
async fn deployed(fx: &AppFx, seen: &[Value]) -> Value {
    let (status, detail) = fx
        .post("/api/project/experiments/try/deploy", json!({}))
        .await;
    assert_eq!(status, 200, "{detail}");
    let wait = std::time::Duration::from_secs(60);
    let new = poll_until(wait, std::time::Duration::from_millis(200), async || {
        given(fx)
            .await
            .into_iter()
            .find(|d| d.is_object() && !seen.contains(d))
    });
    new.await.expect("a run started with the world's data")
}

fn snapshot_text(data: &Value) -> String {
    let dir = PathBuf::from(data["dir"].as_str().unwrap());
    std::fs::read_to_string(dir.join("data/seed.json")).unwrap()
}

/// An app on `project` (on `providers`, when given) into which the fixture repo `p` was imported.
async fn imported(
    p: &str,
    project: &std::path::Path,
    providers: Option<ProviderConfig>,
) -> (AppFx, tempfile::TempDir) {
    let (repo, _) = fixture_repo(p);
    let mut setup = AppFx::idle("worlddata", p).project(project);
    if let Some(providers) = providers {
        setup = setup.providers(providers);
    }
    let fx = setup.start().await;
    let url = repo.path().display().to_string();
    let (status, v) = fx
        .post(
            "/api/project/import-git",
            json!({"url": url, "confirm": true}),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    (fx, repo)
}

/// `json` posted to the world's data route: its status and answer.
async fn put(fx: &AppFx, json: Value) -> (u16, Value) {
    fx.post(DATA, json).await
}

/// The world marks `data/seed.json` and it is uploaded as `{"n": 5}` (replacing the repo's).
async fn upload_seed(fx: &AppFx, project: &std::path::Path, bench: &str) {
    write(
        project,
        "worlds/box/world.yaml",
        &WORLD.replace("wd-bench", bench),
    );
    let (status, put) = put(fx, json!({"path": "data/seed.json", "text": "{\"n\": 5}"})).await;
    assert_eq!(
        (status, put["replaced"].as_bool()),
        (200, Some(true)),
        "{put}"
    );
}

#[test]
fn uploads_reach_the_runs_and_an_edit_is_new_data() {
    if !uv_found() {
        return;
    }
    run(async {
        let project = tempfile::tempdir().unwrap();
        let providers = ProviderConfig::with_upstream(&silent_provider().await);
        let (fx, _repo) = imported("wd", project.path(), Some(providers)).await;
        upload_seed(&fx, project.path(), "wd-bench").await;
        let first = deployed(&fx, &[]).await;
        let file = &first["files"][0];
        assert_eq!(
            (&file["path"], &file["seeds"]),
            (&json!("data/seed.json"), &json!("box"))
        );
        assert_eq!(snapshot_text(&first), "{\"n\": 5}");
        let set = json!({"path": "data/seed.json", "pointer": "/n", "value": 7});
        assert_eq!(put(&fx, set).await.0, 200);
        let second = deployed(&fx, &given(&fx).await).await;
        assert_ne!(second["digest"], first["digest"]);
        let text = snapshot_text(&second);
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap(),
            json!({"n": 7})
        );
        let kept = snapshot_text(&first);
        assert_eq!(
            kept, "{\"n\": 5}",
            "the first run's data is kept for its forks"
        );
    });
}

/// What the data route refuses, and with which status.
fn refused() -> Vec<(Value, u16)> {
    vec![
        (json!({"path": "data/x.json", "text": "{not json"}), 400),
        (json!({"path": "world.yaml", "text": "format: x"}), 400),
        (json!({"path": "../other/x.csv", "text": "a,b"}), 400),
        (json!({"path": "data/x.bin", "data": "AAAA"}), 400),
        (
            json!({"path": "data/big.txt", "text": "x".repeat((10 << 20) + 1)}),
            413,
        ),
    ]
}

#[test]
fn data_files_are_checked_and_deleted() {
    if !uv_found() {
        return;
    }
    run(async {
        let project = tempfile::tempdir().unwrap();
        let (fx, _repo) = imported("wdc", project.path(), None).await;
        for (bad, status) in refused() {
            assert_eq!(put(&fx, bad.clone()).await.0, status, "{}", bad["path"]);
        }
        let csv = json!({"path": "data/rates.csv", "text": "lane,rate\nA,3\n"});
        let (status, answer) = put(&fx, csv).await;
        assert_eq!((status, answer["replaced"].as_bool()), (200, Some(false)));
        let file = format!("{DATA}?path=data/rates.csv");
        let (_, read) = fx.get(&format!("{file}&full=true")).await;
        assert_eq!(read["text"], "lane,rate\nA,3\n");
        assert_eq!(fx.delete(&file).await.0, 200);
        assert!(!project.path().join("worlds/box/data/rates.csv").exists());
        assert_eq!(fx.delete(&file).await.0, 404);
    });
}

/// The checks of the first run of the experiment that was scored, once there is one.
async fn first_checks(fx: &AppFx) -> Option<Vec<Value>> {
    let (_, runs) = fx.get("/api/project/experiments/try/runs").await;
    for branch in branch_ids(&runs) {
        let events = records(&fx.state.store).events(&branch).await;
        let events = events.unwrap_or_default();
        if let Some(e) = events.iter().find(|e| e.kind == EventKind::Check) {
            return e.payload["checks"].as_array().cloned();
        }
    }
    None
}

/// A run whose agent answers at once is scored, and the scoring gets the same data as the parts.
#[test]
fn the_scoring_gets_the_worlds_data() {
    if !uv_found() {
        return;
    }
    run(async {
        let project = tempfile::tempdir().unwrap();
        let llm = fake_llm(vec![(200, gpt_oss("done", Some("All done.")))]).await;
        let providers = Some(ProviderConfig::with_upstream(&llm.base));
        let (fx, _repo) = imported("wds", project.path(), providers).await;
        upload_seed(&fx, project.path(), "wds-bench").await;
        let data = deployed(&fx, &[]).await;
        let (wait, step) = (Duration::from_secs(60), Duration::from_millis(200));
        let checks = poll_until(wait, step, async || first_checks(&fx).await).await;
        let checks = checks.expect("the run was scored");
        let named = checks.iter().find(|c| c["name"] == "world_data");
        assert_eq!(
            named.map(|c| &c["detail"]),
            Some(&data["digest"]),
            "{checks:?}"
        );
    });
}
