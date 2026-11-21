//! The file formats: every fixture round-trips, and malformed or unsafe files are refused with a
//! JSON pointer to the offending value.

use std::path::{Path, PathBuf};

use shakedown_project::format::{
    parse, to_yaml, ExperimentFile, Format, ProjectFile, ScenarioFile, SetupFile, WorldFile,
};

fn fixture(rel: &str) -> (String, String) {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/larkspur")
        .join(rel);
    let text = std::fs::read_to_string(&path).expect("fixture exists");
    (rel.to_string(), text)
}

fn round_trip<T: Format + PartialEq + std::fmt::Debug>(rel: &str) {
    let (file, text) = fixture(rel);
    let first: T = parse(&file, &text).expect("fixture parses");
    let again: T = parse(&file, &to_yaml(&first).expect("serializes")).expect("re-parses");
    assert_eq!(first, again, "{rel} changed in a round trip");
}

#[test]
fn round_trip_each_kind() {
    round_trip::<ProjectFile>("shakedown.yaml");
    round_trip::<WorldFile>("worlds/larkspur-plant/world.yaml");
    round_trip::<ScenarioFile>("scenarios/supplier-slip.yaml");
    round_trip::<SetupFile>("setups/solo-planner.yaml");
    round_trip::<SetupFile>("setups/planning-team.yaml");
    round_trip::<SetupFile>("setups/planner-with-rules.yaml");
    round_trip::<ExperimentFile>("experiments/supplier-slip-solo-vs-team.yaml");
}

#[test]
fn unknown_field_rejected() {
    let text = "format: shakedown-world/3\nlabel: W\ndata:\n  - {database: db, engine: postgres, topology: {kind: single}}\n";
    let err = parse::<WorldFile>("w.yaml", text).unwrap_err();
    assert_eq!(err.pointer, "/data/0/topology", "{err}");
}

#[test]
fn wrong_format_rejected() {
    let err = parse::<WorldFile>("w.yaml", "format: shakedown-world/2\nlabel: W\n").unwrap_err();
    assert_eq!(err.pointer, "/format");
    assert!(err.message.contains("shakedown-world/3"), "{err}");
}

#[test]
fn inline_secret_rejected_without_echo() {
    let text = "format: shakedown-world/3\nlabel: W\nconnectors:\n  - {id: a, label: A, mode: real, auth: {key: \"sk-ant-abc123\"}}\n";
    let err = parse::<WorldFile>("w.yaml", text).unwrap_err();
    assert_eq!(err.pointer, "/connectors/0/auth/key");
    assert!(
        !err.to_string().contains("abc123"),
        "the secret leaked: {err}"
    );
}

#[test]
fn secret_named_field_with_literal_rejected() {
    let text = "format: shakedown-setup/1\nlabel: S\nteam: {kind: http, url: \"https://x\", password: hunter2}\n";
    let err = parse::<SetupFile>("s.yaml", text).unwrap_err();
    assert_eq!(err.pointer, "/team/password");
    assert!(!err.to_string().contains("hunter2"));
}

#[test]
fn key_name_accepted() {
    let text = "format: shakedown-world/3\nlabel: W\nconnectors:\n  - {id: a, label: A, mode: real, auth: {key: larkspur-mes}}\n";
    let world = parse::<WorldFile>("w.yaml", text).expect("a key-store name is fine");
    assert_eq!(
        world.connectors[0]
            .auth
            .as_ref()
            .and_then(|a| a.key.as_deref()),
        Some("larkspur-mes")
    );
}

#[test]
fn data_source_needs_exactly_one_kind() {
    let text = "format: shakedown-world/3\nlabel: W\ndata:\n  - {file: a.csv, database: db}\n";
    let err = parse::<WorldFile>("w.yaml", text).unwrap_err();
    assert_eq!(err.pointer, "/data/0");
}

#[test]
fn malformed_yaml_reported() {
    let err = parse::<WorldFile>("w.yaml", "format: [\n").unwrap_err();
    assert_eq!(err.file, "w.yaml");
}
