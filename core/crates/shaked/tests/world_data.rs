//! A world's own data: uploaded into its folder, set one value at a time, deleted; the files it
//! marks "Every run starts from this" reach every part of its runs (`world_data` in the part's
//! start config, recorded in `RunStart.world`), and an edit is new data with a new digest.

mod support;

use std::path::PathBuf;

use serde_json::{json, Value};
use shaked::model::EventKind;
use shaked::providers::ProviderConfig;
use support::app::silent_provider;
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

#[test]
fn uploads_reach_the_runs_and_an_edit_is_new_data() {
    if !uv_found() {
        return;
    }
    run(async {
        let (repo, _) = fixture_repo("wd");
        let project = tempfile::tempdir().unwrap();
        let fx = AppFx::idle("worlddata", "runs")
            .providers(ProviderConfig::with_upstream(&silent_provider().await))
            .project(project.path())
            .start()
            .await;
        let url = repo.path().display().to_string();
        let (status, v) = fx
            .post(
                "/api/project/import-git",
                json!({"url": url, "confirm": true}),
            )
            .await;
        assert_eq!(status, 200, "{v}");
        write(project.path(), "worlds/box/world.yaml", WORLD);
        let (status, put) = fx
            .post(
                DATA,
                json!({"path": "data/seed.json", "text": "{\"n\": 5}"}),
            )
            .await;
        assert_eq!(
            (status, put["replaced"].as_bool()),
            (200, Some(true)),
            "{put}"
        );
        let first = deployed(&fx, &[]).await;
        assert_eq!(first["files"][0]["path"], "data/seed.json", "{first}");
        assert_eq!(first["files"][0]["seeds"], "box");
        assert_eq!(snapshot_text(&first), "{\"n\": 5}");
        let set = json!({"path": "data/seed.json", "pointer": "/n", "value": 7});
        let (status, v) = fx.post(DATA, set).await;
        assert_eq!(status, 200, "{v}");
        let second = deployed(&fx, &given(&fx).await).await;
        assert_ne!(second["digest"], first["digest"]);
        assert_eq!(
            serde_json::from_str::<Value>(&snapshot_text(&second)).unwrap(),
            json!({"n": 7})
        );
        assert_eq!(
            snapshot_text(&first),
            "{\"n\": 5}",
            "the first run's data is kept for its forks"
        );
    });
}

#[test]
fn data_files_are_checked_and_deleted() {
    if !uv_found() {
        return;
    }
    run(async {
        let (repo, _) = fixture_repo("wdc");
        let project = tempfile::tempdir().unwrap();
        let fx = AppFx::idle("worlddata", "files")
            .project(project.path())
            .start()
            .await;
        let url = repo.path().display().to_string();
        assert_eq!(
            fx.post(
                "/api/project/import-git",
                json!({"url": url, "confirm": true})
            )
            .await
            .0,
            200
        );
        let bad_json = json!({"path": "data/x.json", "text": "{not json"});
        assert_eq!(fx.post(DATA, bad_json).await.0, 400);
        let own = json!({"path": "world.yaml", "text": "format: x"});
        assert_eq!(fx.post(DATA, own).await.0, 400);
        let outside = json!({"path": "../other/x.csv", "text": "a,b"});
        assert_eq!(fx.post(DATA, outside).await.0, 400);
        let binary = json!({"path": "data/x.bin", "data": "AAAA"});
        assert_eq!(fx.post(DATA, binary).await.0, 400);
        let big = "x".repeat((10 << 20) + 1);
        assert_eq!(
            fx.post(DATA, json!({"path": "data/big.txt", "text": big}))
                .await
                .0,
            413
        );
        let csv = json!({"path": "data/rates.csv", "text": "lane,rate\nA,3\n"});
        let (status, put) = fx.post(DATA, csv).await;
        assert_eq!(
            (status, put["replaced"].as_bool()),
            (200, Some(false)),
            "{put}"
        );
        let (_, read) = fx
            .get(&format!("{DATA}?path=data/rates.csv&full=true"))
            .await;
        assert_eq!(read["text"], "lane,rate\nA,3\n");
        let (status, _) = fx.delete(&format!("{DATA}?path=data/rates.csv")).await;
        assert_eq!(status, 200);
        assert!(!project.path().join("worlds/box/data/rates.csv").exists());
        assert_eq!(
            fx.delete(&format!("{DATA}?path=data/rates.csv")).await.0,
            404
        );
    });
}
