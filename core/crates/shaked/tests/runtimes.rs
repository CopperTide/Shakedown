//! Runtimes: installing the packages that run worlds, from a git repo (one folder of it) or an
//! uploaded archive, with no project files; their conformance check; listing; removing (refused
//! while a world uses them); and hosted Shakedown refusing all of it.

mod support;

use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use serde_json::{json, Value};
use support::repo::{fixture_repo, uv_found, write};
use support::{run, AppFx};

async fn app(tag: &str, project: &Path) -> AppFx {
    AppFx::idle("runtimes", tag).project(project).start().await
}

/// The names of the packages a plan or a listing row holds.
fn names(v: &Value) -> Vec<String> {
    let each = v["packages"].as_array().unwrap().iter();
    each.map(|p| p["name"].as_str().unwrap().to_string())
        .collect()
}

/// The repo at `dir` as a `.tar.gz`, base64, its entries under `repo-main/` as GitHub's are.
fn tar_gz_of(dir: &Path) -> String {
    let gz = flate2::write::GzEncoder::new(vec![], flate2::Compression::fast());
    let mut tar = tar::Builder::new(gz);
    for e in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().to_string();
        if name == ".git" {
            continue;
        }
        let at = format!("repo-main/{name}");
        match e.path().is_dir() {
            true => tar.append_dir_all(&at, e.path()).unwrap(),
            false => tar.append_path_with_name(e.path(), &at).unwrap(),
        }
    }
    let bytes = tar.into_inner().unwrap().finish().unwrap();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[test]
fn a_folder_of_a_repo_installs_its_packages_only_and_is_checked() {
    if !uv_found() {
        return;
    }
    run(async {
        let (repo, _) = fixture_repo("rta");
        let project = tempfile::tempdir().unwrap();
        let fx = app("folder", project.path()).await;
        let url = repo.path().display().to_string();
        let asked = json!({"url": url, "path": "parts/toy"});
        let (status, plan) = fx.post("/api/runtimes", asked.clone()).await;
        assert_eq!(status, 200, "{plan}");
        assert_eq!(
            (plan["proposed"].as_bool(), names(&plan)),
            (Some(true), vec!["rta-part".to_string()])
        );
        assert_eq!(
            plan["items"],
            json!([]),
            "a runtime writes no project files"
        );
        let mut confirmed = asked;
        confirmed["confirm"] = json!(true);
        let (status, done) = fx.post("/api/runtimes", confirmed).await;
        assert_eq!(status, 200, "{done}");
        assert!(fx.state.hub.registry().get("rta-part").is_some());
        assert!(
            fx.state.hub.registry().get("rta-bench").is_none(),
            "outside the folder"
        );
        assert!(!project.path().join("worlds").exists());
        let check = &done["checks"][0];
        assert_eq!(check["package"], "rta-part", "{done}");
        assert!(!check["steps"].as_array().unwrap().is_empty(), "{check}");
        let (_, listed) = fx.get("/api/runtimes").await;
        let install = &listed["installs"][0];
        assert_eq!(
            (&install["id"], install["source"].as_str()),
            (&done["id"], Some("git"))
        );
        assert_eq!(
            (install["path"].as_str(), install["runtime"].as_bool()),
            (Some("parts/toy"), Some(true))
        );
        assert_eq!(
            install["packages"][0]["check"]["passed"], check["passed"],
            "{listed}"
        );
        let (status, again) = fx
            .post(
                &format!("/api/runtimes/{}/check", done["id"].as_str().unwrap()),
                json!({}),
            )
            .await;
        assert_eq!(
            (status, again["checks"][0]["package"].as_str()),
            (200, Some("rta-part")),
            "{again}"
        );
    });
}

#[test]
fn an_archive_installs_and_a_world_that_uses_it_keeps_it_installed() {
    if !uv_found() {
        return;
    }
    run(async {
        let (repo, _) = fixture_repo("rtb");
        let project = tempfile::tempdir().unwrap();
        let fx = app("archive", project.path()).await;
        let archive = json!({"name": "desk-main.tar.gz", "data": tar_gz_of(repo.path())});
        let (status, done) = fx
            .post(
                "/api/runtimes",
                json!({"archive": archive, "confirm": true}),
            )
            .await;
        assert_eq!(status, 200, "{done}");
        assert_eq!(
            (done["id"].as_str(), done["source"].as_str()),
            (Some("desk-main"), Some("archive"))
        );
        assert_eq!(
            done["commit"].as_str().map(str::len),
            Some(64),
            "named after its SHA-256"
        );
        assert!(fx.state.hub.bench_describe("rtb-bench").await.is_ok());
        write(
            project.path(),
            "shakedown.yaml",
            "format: shakedown-project/1\nname: Mine\n",
        );
        write(
            project.path(),
            "worlds/mine/world.yaml",
            "format: shakedown-world/3\nlabel: Mine\npackage: rtb-bench\n",
        );
        let (status, refused) = fx.delete("/api/runtimes/desk-main").await;
        assert_eq!(status, 409, "{refused}");
        assert!(
            refused["error"].as_str().unwrap().contains("mine"),
            "{refused}"
        );
        std::fs::remove_dir_all(project.path().join("worlds/mine")).unwrap();
        let (status, removed) = fx.delete("/api/runtimes/desk-main").await;
        assert_eq!(status, 200, "{removed}");
        assert!(fx.state.hub.registry().get("rtb-bench").is_none());
        assert!(!project.path().join(".shakedown/imports/desk-main").exists());
    });
}

#[test]
fn a_bad_archive_and_a_folder_without_packages_are_refused() {
    run(async {
        let project = tempfile::tempdir().unwrap();
        let fx = app("bad", project.path()).await;
        let rar = json!({"archive": {"name": "x.rar", "data": "AAAA"}});
        let (status, v) = fx.post("/api/runtimes", rar).await;
        assert_eq!(status, 400, "{v}");
        let both = json!({"url": "/srv/x", "archive": {"name": "x.zip", "data": ""}});
        assert_eq!(fx.post("/api/runtimes", both).await.0, 400);
        let up = json!({"url": "/srv/x", "path": "../elsewhere"});
        let (status, v) = fx.post("/api/runtimes", up).await;
        assert_eq!(status, 400, "{v}");
        assert_eq!(fx.delete("/api/runtimes/nothing").await.0, 404);
    });
}

/// Hosted Shakedown refuses to install, check or remove a runtime, as it refuses Import from git.
#[test]
fn a_hosted_shaked_refuses_runtimes() {
    run(async {
        let project = tempfile::tempdir().unwrap();
        let hosted = shaked::layer::Policy {
            serves_others: true,
            runs_packages_here: false,
            ..shaked::layer::Policy::LOCAL
        };
        let spy = shaked::harness::SpyLayer::new().with_policy(hosted);
        let fx = AppFx::idle("runtimes", "hosted")
            .layer(Arc::new(spy))
            .project(project.path())
            .start()
            .await;
        let (status, v) = fx
            .post(
                "/api/runtimes",
                json!({"url": "/srv/repo", "confirm": true}),
            )
            .await;
        assert_eq!(status, 403, "{v}");
        assert!(
            v["error"]
                .as_str()
                .unwrap()
                .contains("installing a runtime"),
            "{v}"
        );
        assert_eq!(fx.post("/api/runtimes/x/check", json!({})).await.0, 403);
        assert_eq!(fx.delete("/api/runtimes/x").await.0, 403);
        let (status, listed) = fx.get("/api/runtimes").await;
        assert_eq!(
            (status, listed["hosted"].as_bool()),
            (200, Some(true)),
            "{listed}"
        );
        assert!(!project.path().join(".shakedown").exists());
    });
}
