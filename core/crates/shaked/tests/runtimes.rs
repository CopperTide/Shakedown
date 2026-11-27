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

/// The plan, then the install, of the folder `parts/toy` of the repo at `url`: the install.
async fn install_folder(fx: &AppFx, url: &str) -> Value {
    let asked = json!({"url": url, "path": "parts/toy"});
    let (status, plan) = fx.post("/api/runtimes", asked.clone()).await;
    assert_eq!(status, 200, "{plan}");
    assert_eq!(plan["proposed"], true);
    assert_eq!(names(&plan), ["rta-part"]);
    assert_eq!(
        plan["items"],
        json!([]),
        "a runtime writes no project files"
    );
    let mut confirmed = asked;
    confirmed["confirm"] = json!(true);
    let (status, done) = fx.post("/api/runtimes", confirmed).await;
    assert_eq!(status, 200, "{done}");
    done
}

/// The install `done` is listed with its source, folder and check, and its check runs again.
async fn check_listed(fx: &AppFx, done: &Value) {
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
    let check = &done["checks"][0];
    assert_eq!(
        install["packages"][0]["check"]["passed"], check["passed"],
        "{listed}"
    );
    let again = format!("/api/runtimes/{}/check", done["id"].as_str().unwrap());
    let (status, again) = fx.post(&again, json!({})).await;
    assert_eq!(
        (status, again["checks"][0]["package"].as_str()),
        (200, Some("rta-part")),
        "{again}"
    );
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
        let done = install_folder(&fx, &repo.path().display().to_string()).await;
        let has = |name: &str| fx.state.hub.registry().get(name).is_some();
        assert!(has("rta-part"));
        assert!(!has("rta-bench"), "outside the folder");
        assert!(!project.path().join("worlds").exists());
        let check = &done["checks"][0];
        assert_eq!(check["package"], "rta-part", "{done}");
        assert!(!check["steps"].as_array().unwrap().is_empty(), "{check}");
        check_listed(&fx, &done).await;
    });
}

/// The repo at `repo` installed as the archive `desk-main.tar.gz`: the install.
async fn install_archive(fx: &AppFx, repo: &Path) -> Value {
    let archive = json!({"name": "desk-main.tar.gz", "data": tar_gz_of(repo)});
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
    done
}

/// A world of the project at `p` runs on `rtb-bench`: removing its install is refused, naming it;
/// once the world is gone, the install is removed.
async fn removed_once_unused(fx: &AppFx, p: &Path) {
    write(
        p,
        "shakedown.yaml",
        "format: shakedown-project/1\nname: Mine\n",
    );
    write(
        p,
        "worlds/mine/world.yaml",
        "format: shakedown-world/3\nlabel: Mine\npackage: rtb-bench\n",
    );
    let (status, refused) = fx.delete("/api/runtimes/desk-main").await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("mine"),
        "{refused}"
    );
    std::fs::remove_dir_all(p.join("worlds/mine")).unwrap();
    let (status, removed) = fx.delete("/api/runtimes/desk-main").await;
    assert_eq!(status, 200, "{removed}");
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
        install_archive(&fx, repo.path()).await;
        assert!(fx.state.hub.bench_describe("rtb-bench").await.is_ok());
        removed_once_unused(&fx, project.path()).await;
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

/// An app on `project` whose layer serves others, as hosted Shakedown's does.
async fn hosted_app(project: &Path) -> AppFx {
    let hosted = shaked::layer::Policy {
        serves_others: true,
        runs_packages_here: false,
        ..shaked::layer::Policy::LOCAL
    };
    let spy = shaked::harness::SpyLayer::new().with_policy(hosted);
    AppFx::idle("runtimes", "hosted")
        .layer(Arc::new(spy))
        .project(project)
        .start()
        .await
}

/// Hosted Shakedown refuses to install, check or remove a runtime, as it refuses Import from git.
#[test]
fn a_hosted_shaked_refuses_runtimes() {
    run(async {
        let project = tempfile::tempdir().unwrap();
        let fx = hosted_app(project.path()).await;
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
