//! Service tests that need no Docker: config checks, ids and the part's description.
use serde_json::{json, Value};
use shakedown_part::proto::*;
use shakedown_part::Part;
use shakedown_service::Service;
use std::path::PathBuf;

fn part() -> Service {
    Service::new(
        std::env::temp_dir().join(format!("shakedown_service_nodocker_{}", std::process::id())),
    )
}

fn start(instance: &str, config: Value) -> StartParams {
    StartParams {
        instance: instance.into(),
        config,
        seed: 1,
        clock_ms: 0,
        from: None,
        world_time: None,
        models: None,
    }
}

#[tokio::test]
async fn describe_is_exact_and_toolless() {
    let d = part().describe().await.unwrap();
    assert_eq!(
        (d.restore.exact, d.restore.approximate),
        (ExactKind::Save, false)
    );
    assert!(d.tools.is_empty());
    assert_eq!(d.name, "service");
}

#[tokio::test]
async fn bad_config_is_invalid_params() {
    let p = part();
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let bad = [
        json!({}),
        json!({"compose_file": here, "volumes": ["../x"]}),
        json!({"compose_file": here, "volumes": ["a b"]}),
        json!({"compose_file": "/nonexistent/compose.yml", "volumes": ["data"]}),
        json!({"compose_file": here, "volumes": "data"}),
    ];
    for config in bad {
        let e = p.start(start("a", config.clone())).await.unwrap_err();
        assert_eq!(e.code, -32602, "{config}");
    }
}

#[tokio::test]
async fn unknown_instance_and_save() {
    let p = part();
    let e = p
        .stop(IdParams {
            instance: "a".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(e.code, -32002);
    let e = p
        .save(SaveParams {
            instance: "a".into(),
            save: "s".into(),
            dir: "/nonexistent".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(e.code, -32002);
    let e = p
        .inspect(InspectParams {
            target: Target::Save { save: "s".into() },
            keys: vec![],
        })
        .await
        .unwrap_err();
    assert_eq!(e.code, -32002);
    assert_eq!(p.usage().await.unwrap().bytes, 0);
}

#[tokio::test]
async fn volume_strategies_that_lack_what_they_need_are_refused() {
    let p = part();
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let pg = json!({"compose_file": here, "volumes": [{"name": "pg", "save": "postgres"}]});
    let e = p.start(start("a", pg)).await.unwrap_err();
    assert_eq!(e.code, -32602);
    assert_eq!(
        e.message,
        "volume pg saves by postgres, which needs service (the compose service running the database)"
    );
    let bad_way = json!({"compose_file": here, "volumes": [{"name": "x", "save": "zip"}]});
    assert_eq!(p.start(start("a", bad_way)).await.unwrap_err().code, -32602);
}
