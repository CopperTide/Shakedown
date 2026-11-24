//! The operation registry: every operation is named, typed and runnable, and edits show a diff
//! before anything is written unless the project allows otherwise.

mod support;

use serde_json::{json, Value};
use shaked::ops::{self, Access};
use support::project::{project_app, ProjectFx};
use support::{records, run, AppFx};

async fn call(fx: &AppFx, name: &str, input: Value) -> Result<Value, ops::OpError> {
    let op = ops::find(name).unwrap_or_else(|| panic!("no operation {name}"));
    (op.run)(&ops::OpCtx::of(&fx.state), op, input).await
}

fn scenario_body(label: &str) -> Value {
    json!({
        "format": "shakedown-scenario/1",
        "label": label,
        "world": "larkspur-plant",
        "task": "Keep SO-1001 on schedule.",
    })
}

#[test]
fn names_unique_and_dotted() {
    let mut seen = std::collections::HashSet::new();
    for op in ops::all() {
        let (area, verb) = op.name.split_once('.').expect("dotted");
        assert!(area.chars().all(|c| c.is_ascii_lowercase()), "{}", op.name);
        assert!(
            verb.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{}",
            op.name
        );
        assert!(seen.insert(op.name), "{} twice", op.name);
    }
    assert!(ops::find("scenarios.put").is_some());
}

#[test]
fn schemas_are_objects() {
    for op in ops::all() {
        let schema = (op.input_schema)();
        assert_eq!(
            schema["type"], "object",
            "{} input schema: {schema}",
            op.name
        );
    }
}

#[test]
fn access_classes_follow_the_verb() {
    for op in ops::all() {
        let want = match op.name.rsplit('.').next().unwrap() {
            "list" | "get" | "status" | "runs" | "madl" | "dependents" | "deleted" | "data" => {
                Access::Read
            }
            "delete" => Access::Delete,
            "deploy" => Access::Spend,
            _ => Access::Edit,
        };
        assert_eq!(op.access, want, "{}", op.name);
    }
}

#[test]
fn list_and_get() {
    run(async {
        let fx = project_app("ops", "list").await;
        let listed = call(&fx, "setups.list", json!({})).await.unwrap();
        assert_eq!(listed["items"].as_array().unwrap().len(), 3);
        let got = call(&fx, "scenarios.get", json!({"id": "supplier-slip"}))
            .await
            .unwrap();
        assert_eq!(got["label"], "A key supplier slips a week");
        assert_eq!(got["kind"], "scenario");
        let missing = call(&fx, "scenarios.get", json!({"id": "nope"}))
            .await
            .unwrap_err();
        assert_eq!(missing.status.as_u16(), 404);
    });
}

#[test]
fn put_diff_first_does_not_write() {
    run(async {
        let fx = project_app("ops", "diff").await;
        let path = fx.dir.path().join("scenarios/supplier-slip.yaml");
        let before = std::fs::read_to_string(&path).unwrap();
        let input = json!({"id": "supplier-slip", "body": scenario_body("Renamed")});
        let out = call(&fx, "scenarios.put", input).await.unwrap();
        assert_eq!(out["proposed"], true);
        assert!(
            out["diff"].as_str().unwrap().contains("+label: Renamed"),
            "{out}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    });
}

#[test]
fn put_confirm_writes() {
    run(async {
        let fx = project_app("ops", "confirm").await;
        let input = json!({"id": "new-one", "body": scenario_body("New"), "confirm": true});
        let out = call(&fx, "scenarios.put", input).await.unwrap();
        assert_eq!(out["label"], "New");
        assert!(fx.dir.path().join("scenarios/new-one.yaml").is_file());
    });
}

#[test]
fn put_rejects_inline_secrets() {
    run(async {
        let fx = project_app("ops", "secret").await;
        let body = json!({"format": "shakedown-setup/1", "label": "S",
                          "team": {"kind": "http", "url": "https://x", "auth": {"key": "sk-ant-zzz999"}}});
        let input = json!({"id": "s", "body": body, "confirm": true});
        let err = call(&fx, "setups.put", input).await.unwrap_err();
        assert_eq!(err.status.as_u16(), 422);
        assert!(!err.message.contains("zzz999"));
    });
}

#[test]
fn bad_input_is_400() {
    run(async {
        let fx = project_app("ops", "bad").await;
        let err = call(&fx, "scenarios.get", json!({})).await.unwrap_err();
        assert_eq!(err.status.as_u16(), 400);
        let err = call(&fx, "scenarios.get", json!({"id": "../x"}))
            .await
            .unwrap_err();
        assert_eq!(err.status.as_u16(), 422);
    });
}

#[test]
fn delete_removes_file() {
    run(async {
        let fx = project_app("ops", "delete").await;
        call(
            &fx,
            "experiments.delete",
            json!({"id": "supplier-slip-solo-vs-team"}),
        )
        .await
        .unwrap();
        assert!(!fx
            .dir
            .path()
            .join("experiments/supplier-slip-solo-vs-team.yaml")
            .exists());
    });
}

#[test]
fn status_counts_and_problems() {
    run(async {
        let fx = project_app("ops", "status").await;
        let status = call(&fx, "project.status", json!({})).await.unwrap();
        assert_eq!(status["name"], "Larkspur Ops");
        assert_eq!(status["counts"]["setups"], 3);
        assert_eq!(status["problems"].as_array().unwrap().len(), 0);
    });
}

#[test]
fn no_project_is_409() {
    run(async {
        let fx = AppFx::idle("ops", "none").start().await;
        let err = call(&fx, "worlds.list", json!({})).await.unwrap_err();
        assert_eq!(err.status.as_u16(), 409);
    });
}

#[test]
fn init_creates_a_project() {
    run(async {
        let dir = tempfile::tempdir().unwrap();
        let fx = AppFx::idle("ops", "init").project(dir.path()).start().await;
        call(&fx, "project.init", json!({"name": "Fresh", "git": false}))
            .await
            .unwrap();
        let status = call(&fx, "project.status", json!({})).await.unwrap();
        assert_eq!(status["name"], "Fresh");
        let again = call(&fx, "project.init", json!({"name": "Again", "git": false})).await;
        assert_eq!(again.unwrap_err().status.as_u16(), 409);
    });
}

#[test]
fn http_put_then_get_round_trips() {
    run(async {
        let fx = project_app("ops", "http").await;
        let url = format!("{}/api/project/scenarios/from-http", fx.base);
        let body = json!({"body": scenario_body("Over HTTP"), "confirm": true});
        let put = fx.http.put(&url).json(&body).send().await.unwrap();
        assert_eq!(put.status().as_u16(), 200);
        let (status, got) = fx.get("/api/project/scenarios/from-http").await;
        assert_eq!(status, 200, "{got}");
        assert_eq!(got["label"], "Over HTTP");
        let (status, listed) = fx.get("/api/project/scenarios").await;
        assert_eq!(status, 200);
        assert_eq!(listed["items"].as_array().unwrap().len(), 2);
    });
}

#[test]
fn http_errors_keep_their_status() {
    run(async {
        let fx = AppFx::idle("ops", "http409").start().await;
        let (status, _) = fx.get("/api/project").await;
        assert_eq!(status, 409);
    });
}

fn settings(edit: &str) -> Value {
    json!({"format": "shakedown-project/1", "name": "Larkspur Ops", "permissions": {"edit": edit, "spend": "always_ask"}})
}

#[test]
fn update_settings_diff_first() {
    run(async {
        let fx = project_app("ops", "settings").await;
        let out = call(
            &fx,
            "project.update",
            json!({"body": settings("apply_then_tell")}),
        )
        .await
        .unwrap();
        assert_eq!(out["proposed"], true, "{out}");
        assert!(
            out["diff"]
                .as_str()
                .unwrap()
                .contains("+  edit: apply_then_tell"),
            "{out}"
        );
        let status = call(&fx, "project.status", json!({})).await.unwrap();
        assert_eq!(status["settings"]["permissions"]["edit"], "diff_first");
    });
}

#[test]
fn apply_then_tell_writes_directly() {
    run(async {
        let fx = project_app("ops", "tell").await;
        call(
            &fx,
            "project.update",
            json!({"body": settings("apply_then_tell"), "confirm": true}),
        )
        .await
        .unwrap();
        let input = json!({"id": "direct", "body": scenario_body("Direct")});
        let out = call(&fx, "scenarios.put", input).await.unwrap();
        assert_eq!(out["label"], "Direct", "{out}");
        assert!(fx.dir.path().join("scenarios/direct.yaml").is_file());
    });
}

/// `PUT /api/project` as `token`: the status.
async fn put_settings(fx: &AppFx, token: &str) -> u16 {
    let body = json!({"body": settings("apply_then_tell"), "confirm": true});
    let req = fx.http.put(format!("{}/api/project", fx.base));
    let res = req.bearer_auth(token).json(&body).send().await.unwrap();
    res.status().as_u16()
}

#[test]
fn update_needs_admin_token() {
    run(async {
        let fx = project_app("ops", "settings-token").await;
        let runner = fx.mint_token(&["read", "run"]).await;
        assert_eq!(put_settings(&fx, &runner).await, 403);
        let admin = fx.mint_token(&["read", "run", "admin"]).await;
        assert_eq!(put_settings(&fx, &admin).await, 200);
    });
}

const EXPERIMENT: &str = "supplier-slip-solo-vs-team";

#[test]
fn delete_in_use_is_409_listing_dependents() {
    run(async {
        let fx = project_app("ops", "inuse").await;
        let url = format!("{}/api/project/scenarios/supplier-slip", fx.base);
        let res = fx.http.delete(&url).send().await.unwrap();
        assert_eq!(res.status().as_u16(), 409);
        let body: Value = res.json().await.unwrap();
        let want = json!([{"kind": "experiment", "id": EXPERIMENT,
                           "label": "Supplier slip: solo vs team", "fate": "deleted",
                           "fates": ["deleted"]}]);
        assert_eq!(body["dependents"], want, "{body}");
        assert!(
            body["error"].as_str().unwrap().contains(EXPERIMENT),
            "{body}"
        );
        assert!(fx.dir.path().join("scenarios/supplier-slip.yaml").is_file());
    });
}

#[test]
fn delete_cascade_over_http_takes_dependents_too() {
    run(async {
        let fx = project_app("ops", "cascade").await;
        let url = format!("{}/api/project/worlds/larkspur-plant", fx.base);
        let res = fx
            .http
            .delete(&url)
            .json(&json!({"cascade": true, "confirm": true}))
            .send()
            .await;
        let res = res.unwrap();
        assert_eq!(res.status().as_u16(), 200);
        let out: Value = res.json().await.unwrap();
        assert_eq!(out["deleted"], "larkspur-plant");
        assert_eq!(out["with"].as_array().unwrap().len(), 2, "{out}");
        assert!(!fx.dir.path().join("scenarios/supplier-slip.yaml").exists());
        assert!(
            fx.dir.path().join("worlds/larkspur-plant").is_dir(),
            "the data folder stays"
        );
        let status = call(&fx, "project.status", json!({})).await.unwrap();
        assert_eq!(status["problems"], json!([]));
    });
}

#[test]
fn dependents_preview_names_each_fate() {
    run(async {
        let fx = project_app("ops", "dependents").await;
        let out = call(&fx, "setups.dependents", json!({"id": "solo-planner"}))
            .await
            .unwrap();
        assert_eq!(out["dependents"][0]["fate"], "setup_dropped", "{out}");
        let (status, out) = fx
            .get("/api/project/worlds/larkspur-plant/dependents")
            .await;
        assert_eq!(status, 200, "{out}");
        let kinds: Vec<&str> = out["dependents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["scenario", "experiment"]);
        let none = call(&fx, "experiments.dependents", json!({"id": EXPERIMENT})).await;
        assert_eq!(none.unwrap()["dependents"], json!([]));
    });
}

#[test]
fn list_summaries_say_where_items_belong() {
    run(async {
        let fx = project_app("ops", "belong").await;
        let worlds = call(&fx, "worlds.list", json!({})).await.unwrap();
        assert_eq!(worlds["items"][0]["group"], Value::Null);
        let scenarios = call(&fx, "scenarios.list", json!({})).await.unwrap();
        assert_eq!(scenarios["items"][0]["world"], "larkspur-plant");
        let experiments = call(&fx, "experiments.list", json!({})).await.unwrap();
        assert_eq!(experiments["items"][0]["scenario"], "supplier-slip");
        assert_eq!(experiments["items"][0]["world"], "larkspur-plant");
    });
}

/// Adds worlds `ids`, each in `group`.
async fn put_worlds_in(fx: &AppFx, group: &str, ids: &[&str]) {
    for id in ids {
        let body = json!({"format": "shakedown-world/3", "label": id, "group": group});
        let input = json!({"id": id, "body": body, "confirm": true});
        call(fx, "worlds.put", input).await.unwrap();
    }
}

/// The `group` of each world, in id order.
async fn groups(fx: &AppFx) -> Vec<Value> {
    let worlds = call(fx, "worlds.list", json!({})).await.unwrap();
    let items = worlds["items"].as_array().unwrap();
    items.iter().map(|w| w["group"].clone()).collect()
}

#[test]
fn rename_group_shows_a_diff_then_moves_every_world() {
    run(async {
        let fx = project_app("ops", "group").await;
        put_worlds_in(&fx, "Acme", &["acme-east", "acme-west"]).await;
        let ask = json!({"from": "Acme", "to": "Acme Foods"});
        let proposed = call(&fx, "worlds.rename_group", ask.clone()).await.unwrap();
        assert_eq!(proposed["proposed"], true);
        let diff = proposed["diff"].as_str().unwrap();
        assert!(diff.contains("+group: Acme Foods"), "{proposed}");
        assert_eq!(groups(&fx).await[0], "Acme", "a proposal writes nothing");
        let mut confirmed = ask;
        confirmed["confirm"] = json!(true);
        let out = call(&fx, "worlds.rename_group", confirmed).await.unwrap();
        assert_eq!(out["moved"], json!(["acme-east", "acme-west"]));
        let foods = json!("Acme Foods");
        assert_eq!(groups(&fx).await, [foods.clone(), foods, Value::Null]);
        let missing = json!({"from": "Nobody", "to": "X", "confirm": true});
        let err = call(&fx, "worlds.rename_group", missing).await.unwrap_err();
        assert_eq!(err.status.as_u16(), 404);
    });
}

/// Sends a delete over HTTP: its status and answer.
async fn http_delete(fx: &AppFx, path: &str, body: Value) -> (u16, Value) {
    let url = format!("{}{path}", fx.base);
    let res = fx.http.delete(&url).json(&body).send().await.unwrap();
    (res.status().as_u16(), res.json().await.unwrap())
}

#[test]
fn a_stale_preview_is_409_with_the_fresh_dependents() {
    run(async {
        let fx = project_app("ops", "stale").await;
        let shown = call(&fx, "setups.dependents", json!({"id": "solo-planner"}))
            .await
            .unwrap();
        let body = json!({"id": "solo-only", "body": {"format": "shakedown-experiment/1",
            "label": "Solo only", "world": "larkspur-plant", "scenario": "supplier-slip",
            "setups": ["solo-planner"], "repeats": {"per_setup": 1}}, "confirm": true});
        call(&fx, "experiments.put", body).await.unwrap();
        let ask = json!({"cascade": true, "confirm": true, "expect": shown["dependents"]});
        let (status, out) = http_delete(&fx, "/api/project/setups/solo-planner", ask).await;
        assert_eq!(status, 409, "{out}");
        assert_eq!(out["dependents"].as_array().unwrap().len(), 2, "{out}");
        assert!(fx.dir.path().join("setups/solo-planner.yaml").is_file());
        let fresh = json!({"cascade": true, "confirm": true, "expect": out["dependents"]});
        let (status, out) = http_delete(&fx, "/api/project/setups/solo-planner", fresh).await;
        assert_eq!(status, 200, "{out}");
    });
}

#[test]
fn diff_first_holds_a_cascade_until_confirmed() {
    run(async {
        let fx = project_app("ops", "hold").await;
        let path = "/api/project/scenarios/supplier-slip";
        let (status, out) = http_delete(&fx, path, json!({"cascade": true})).await;
        assert_eq!(status, 200, "{out}");
        assert_eq!(out["proposed"], true);
        assert_eq!(out["dependents"][0]["id"], EXPERIMENT);
        assert!(fx.dir.path().join("scenarios/supplier-slip.yaml").is_file());
    });
}

#[test]
fn cascade_reads_from_the_query_string() {
    run(async {
        let fx = project_app("ops", "query").await;
        let url = format!(
            "{}/api/project/scenarios/supplier-slip?cascade=true&confirm=true",
            fx.base
        );
        let res = fx.http.delete(&url).send().await.unwrap();
        assert_eq!(res.status().as_u16(), 200);
        assert!(!fx.dir.path().join("scenarios/supplier-slip.yaml").exists());
        let url = format!("{}/api/project/experiments/x?cascade=maybe", fx.base);
        assert_eq!(
            fx.http.delete(&url).send().await.unwrap().status().as_u16(),
            400
        );
    });
}

/// Deletes the example world with everything that uses it: the trash its files went to.
async fn trashed_world(fx: &ProjectFx) -> String {
    let ask = json!({"id": "larkspur-plant", "cascade": true, "confirm": true});
    let out = call(fx, "worlds.delete", ask).await.unwrap();
    let trash = out["trash"].as_str().unwrap().to_string();
    let bin = fx.dir.path().join(".shakedown/trash").join(&trash);
    assert!(bin.join("worlds/larkspur-plant/world.yaml").is_file());
    trash
}

#[test]
fn undo_puts_a_delete_back() {
    run(async {
        let fx = project_app("ops", "undo").await;
        let trash = trashed_world(&fx).await;
        let proposed = call(&fx, "project.restore", json!({})).await.unwrap();
        let diff = proposed["diff"].as_str().unwrap();
        assert!(
            diff.contains("+worlds/larkspur-plant/world.yaml"),
            "{proposed}"
        );
        let ask = json!({"trash": trash, "confirm": true});
        let out = call(&fx, "project.restore", ask).await.unwrap();
        assert_eq!(out["restored"], "world larkspur-plant");
        let status = call(&fx, "project.status", json!({})).await.unwrap();
        assert_eq!(status["counts"]["scenarios"], 1);
        assert_eq!(status["counts"]["experiments"], 1);
        let again = call(&fx, "project.restore", json!({"confirm": true})).await;
        assert_eq!(again.unwrap_err().status.as_u16(), 404);
    });
}

/// An engine experiment deployed from project experiment `of`: its id.
async fn deployment_of(fx: &AppFx, of: &str) -> String {
    let team: shaked::model::Team = serde_json::from_value(json!({"id": "t", "name": "T",
        "agents": [{"id": "a", "name": "A", "kind": "open", "model": "m", "prompt": "p"}]}))
    .unwrap();
    let setups = [shaked::store::NewSetup {
        name: "A",
        team: &team,
        runner: shaked::model::Runner::Chain,
        connection: None,
    }];
    let scenario = Default::default();
    let new = shaked::store::NewExperiment {
        name: "Old",
        task: "t1",
        runs: 1,
        benchmark: "toybench",
        scenario: &scenario,
        setups: &setups,
        max_cost_usd: None,
        seed: 1,
        world_minutes: None,
        project_experiment: Some(of),
    };
    records(&fx.state.store)
        .create_experiment(new)
        .await
        .unwrap()
        .0
        .id
}

#[test]
fn a_deleted_experiment_lets_go_of_its_runs() {
    run(async {
        let fx = project_app("ops", "unlink").await;
        let deployed = deployment_of(&fx, EXPERIMENT).await;
        let ask = json!({"id": EXPERIMENT});
        let out = call(&fx, "experiments.delete", ask).await.unwrap();
        assert_eq!(out["runs_unlinked"], true);
        let store = records(&fx.state.store);
        assert!(store.deployments_of(EXPERIMENT).await.unwrap().is_empty());
        assert!(store
            .unlinked_experiments()
            .await
            .unwrap()
            .contains(&deployed));
        let ask = json!({"trash": out["trash"], "confirm": true});
        let back = call(&fx, "project.restore", ask).await.unwrap();
        assert_eq!(back["runs_relinked"], true);
        let linked = store.deployments_of(EXPERIMENT).await.unwrap();
        assert_eq!(linked, [deployed], "undo links the runs again");
    });
}

#[test]
fn a_restore_proposal_restores_its_own_delete() {
    run(async {
        let fx = project_app("ops", "pinned").await;
        call(&fx, "experiments.delete", json!({"id": EXPERIMENT}))
            .await
            .unwrap();
        let last = call(&fx, "project.deleted", json!({})).await.unwrap();
        assert_eq!(last["label"], format!("experiment {EXPERIMENT}"));
        let proposal = call(&fx, "project.restore", json!({})).await.unwrap();
        assert_eq!(proposal["trash"], last["trash"]);
        call(&fx, "setups.delete", json!({"id": "planner-with-rules"}))
            .await
            .unwrap();
        let ask = json!({"trash": proposal["trash"], "confirm": true});
        let out = call(&fx, "project.restore", ask).await.unwrap();
        assert_eq!(out["restored"], format!("experiment {EXPERIMENT}"));
        assert!(!fx
            .dir
            .path()
            .join("setups/planner-with-rules.yaml")
            .exists());
    });
}

#[test]
fn undo_after_a_later_edit_is_409() {
    run(async {
        let fx = project_app("ops", "edited").await;
        let ask = json!({"id": "solo-planner", "cascade": true, "confirm": true});
        call(&fx, "setups.delete", ask).await.unwrap();
        let mut item = call(&fx, "experiments.get", json!({"id": EXPERIMENT}))
            .await
            .unwrap();
        item["body"]["label"] = json!("Edited since");
        let put = json!({"id": EXPERIMENT, "body": item["body"], "confirm": true});
        call(&fx, "experiments.put", put).await.unwrap();
        let err = call(&fx, "project.restore", json!({"confirm": true}))
            .await
            .unwrap_err();
        assert_eq!(err.status.as_u16(), 409);
        let file = format!("experiments/{EXPERIMENT}.yaml");
        assert!(err.message.contains(&file), "{}", err.message);
        let now = call(&fx, "experiments.get", json!({"id": EXPERIMENT}))
            .await
            .unwrap();
        assert_eq!(now["label"], "Edited since");
    });
}

/// `worlds.data` on the example world, after `files` (path, bytes) were put in its data folder.
async fn world_data(files: &[(&str, &[u8])], input: Value) -> Result<Value, ops::OpError> {
    let fx = project_app("ops", "data").await;
    let folder = fx.dir.path().join("worlds/larkspur-plant");
    for (path, bytes) in files {
        std::fs::create_dir_all(folder.join(path).parent().unwrap()).unwrap();
        std::fs::write(folder.join(path), bytes).unwrap();
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(fx.dir.path().join("shakedown.yaml"), folder.join("out")).unwrap();
    let mut input = input;
    input["id"] = json!("larkspur-plant");
    call(&fx, "worlds.data", input).await
}

#[test]
fn world_data_lists_files_recursively() {
    run(async {
        let files: [(&str, &[u8]); 2] = [("b.csv", b"x"), ("imports/a.txt", b"hello")];
        let out = world_data(&files, json!({})).await.unwrap();
        let paths: Vec<&str> = out["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, ["b.csv", "imports/a.txt", "world.yaml"], "{out}");
    });
}

#[test]
fn world_data_reads_text_binary_and_truncates() {
    run(async {
        let big = vec![b'a'; (1 << 20) + 10];
        let files: [(&str, &[u8]); 3] = [("t.txt", b"hi"), ("b.bin", b"a\0b"), ("big.txt", &big)];
        let read = |p: &str| world_data(&files, json!({ "path": p }));
        let text = read("t.txt").await.unwrap();
        assert_eq!(
            text,
            json!({"path": "t.txt", "size": 2, "text": "hi", "truncated": false})
        );
        let bin = read("b.bin").await.unwrap();
        assert_eq!(bin, json!({"path": "b.bin", "size": 3, "binary": true}));
        let cut = read("big.txt").await.unwrap();
        assert_eq!(cut["truncated"], true);
        assert_eq!(cut["text"].as_str().unwrap().len(), 1 << 20);
        assert_eq!(read("missing.txt").await.unwrap_err().status.as_u16(), 404);
    });
}

#[test]
fn world_data_refuses_paths_outside_the_folder() {
    run(async {
        for path in ["../../shakedown.yaml", "/etc/hostname", "out"] {
            let err = world_data(&[("t.txt", b"hi")], json!({ "path": path }))
                .await
                .unwrap_err();
            assert_eq!(err.status.as_u16(), 400, "{path}");
        }
    });
}
