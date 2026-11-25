//! The file formats: every fixture round-trips, and malformed or unsafe files are refused with a
//! JSON pointer to the offending value.

use std::path::{Path, PathBuf};

use shakedown_project::format::{
    parse, parse_duration_ms, to_yaml, ClockMode, ClockSpec, ExperimentFile, Format, ProjectFile,
    ScenarioFile, SetupFile, WorldFile,
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

#[test]
fn world_package_round_trips() {
    let text = "format: shakedown-world/3\nlabel: W\npackage: toybench\n";
    let world = parse::<WorldFile>("w.yaml", text).expect("package parses");
    assert_eq!(world.package.as_deref(), Some("toybench"));
    let again: WorldFile = parse("w.yaml", &to_yaml(&world).unwrap()).unwrap();
    assert_eq!(world, again);
}

/// A world file without a group loads with none and saves without one, the same bytes every time.
#[test]
fn world_without_group_saves_unchanged() {
    let (file, text) = fixture("worlds/larkspur-plant/world.yaml");
    let world: WorldFile = parse(&file, &text).unwrap();
    assert_eq!(world.group, None);
    let written = to_yaml(&world).unwrap();
    assert!(!written.contains("group"), "{written}");
    let again: WorldFile = parse(&file, &written).unwrap();
    assert_eq!(
        to_yaml(&again).unwrap(),
        written,
        "a second save changed the bytes"
    );
}

#[test]
fn world_group_round_trips() {
    let text = "format: shakedown-world/3\ngroup: Acme Foods\nlabel: W\n";
    let world = parse::<WorldFile>("w.yaml", text).expect("group parses");
    assert_eq!(world.group.as_deref(), Some("Acme Foods"));
    assert_eq!(to_yaml(&world).unwrap(), text);
}

#[test]
fn padded_group_rejected() {
    let text = "format: shakedown-world/3\nlabel: W\ngroup: \" Acme\"\n";
    let err = parse::<WorldFile>("w.yaml", text).unwrap_err();
    assert_eq!(err.pointer, "/group", "{err}");
}

#[test]
fn blank_group_rejected() {
    let text = "format: shakedown-world/3\nlabel: W\ngroup: \"  \"\n";
    let err = parse::<WorldFile>("w.yaml", text).unwrap_err();
    assert_eq!(err.pointer, "/group", "{err}");
}

#[test]
fn scenario_preset_round_trips() {
    let text = "format: shakedown-scenario/1\nlabel: S\nworld: w\ntask: T\npreset: t1\n";
    let scenario = parse::<ScenarioFile>("s.yaml", text).expect("preset parses");
    assert_eq!(scenario.preset.as_deref(), Some("t1"));
}

#[test]
fn assistant_settings_round_trip() {
    let text = "format: shakedown-project/1\nname: P\nassistant:\n  model: claude-sonnet\n  provider: anthropic\n";
    let project = parse::<ProjectFile>("shakedown.yaml", text).expect("assistant settings parse");
    let assistant = project.assistant.as_ref().expect("set");
    assert_eq!(
        (assistant.model.as_str(), assistant.provider.as_deref()),
        ("claude-sonnet", Some("anthropic"))
    );
    let again: ProjectFile = parse("shakedown.yaml", &to_yaml(&project).unwrap()).unwrap();
    assert_eq!(project, again);
}

const OBS_WORLD: &str = "format: shakedown-world/3
label: W
observability:
  tools:
    - {id: datadog, product: datadog, mode: stand-in, seeded_from: monitors.json}
    - {id: otel, product: otel-collector, mode: real, auth: {key: otel-token}}
  alerts:
    - {name: PO approval queue over 3, when: approvals.pending > 3, routes_to: planner}
  operators:
    - {id: planner, label: Planner on call, role: planner, behavior: {answers_after: {minutes: 15}}}
    - {id: it, label: IT on call, behavior: never_answers}
  human:
    - {moment: paged, then: continue}
    - {moment: agent_stuck, then: fail}
";

#[test]
fn observability_round_trips() {
    let world = parse::<WorldFile>("w.yaml", OBS_WORLD).expect("observability parses");
    let obs = world.observability.as_ref().unwrap();
    assert_eq!(obs.tools.len(), 2);
    assert_eq!(obs.operators.len(), 2);
    let again: WorldFile = parse("w.yaml", &to_yaml(&world).unwrap()).unwrap();
    assert_eq!(world, again);
}

#[test]
fn bad_operator_behavior() {
    let text = OBS_WORLD.replace("behavior: never_answers", "behavior: sometimes");
    let err = parse::<WorldFile>("w.yaml", &text).unwrap_err();
    assert!(
        err.pointer.starts_with("/observability/operators/1"),
        "{err}"
    );
}

#[test]
fn spend_cap_round_trips() {
    let text = "format: shakedown-project/1\nname: P\npermissions:\n  spend:\n    under_cap: {usd_per_day: 10}\n";
    let project = parse::<ProjectFile>("shakedown.yaml", text).unwrap();
    let again: ProjectFile = parse("shakedown.yaml", &to_yaml(&project).unwrap()).unwrap();
    assert_eq!(project, again);
}

fn stepped_storm() -> String {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenarios/stepped-storm.yaml");
    std::fs::read_to_string(path).expect("fixture exists")
}

#[test]
fn parses_stepped_clock_and_timed_events() {
    let scenario =
        parse::<ScenarioFile>("s.yaml", &stepped_storm()).expect("stepped scenario parses");
    let clock = scenario.clock.as_ref().expect("clock set");
    assert_eq!(clock.mode, ClockMode::Stepped);
    assert_eq!(clock.epoch.as_deref(), Some("60m"));
    assert_eq!(clock.horizon.as_deref(), Some("23:59"));
    assert_eq!(
        clock
            .action_cost
            .tools
            .get("plan_recovery")
            .map(String::as_str),
        Some("5m")
    );
    let event = &scenario.events[0];
    assert_eq!(event.known_at.as_deref(), Some("14:40"));
    assert_eq!(event.part.as_deref(), Some("driver"));
    assert_eq!(
        event.do_.as_ref().and_then(|d| d["kind"].as_str()),
        Some("capacity_program")
    );
    assert_eq!(scenario.generators.len(), 1);
    let again: ScenarioFile = parse("s.yaml", &to_yaml(&scenario).unwrap()).expect("re-parses");
    assert_eq!(scenario, again);
}

#[test]
fn rejects_bad_duration() {
    let text = stepped_storm().replace("write: 2m", "write: \"2 parsecs\"");
    let err = parse::<ScenarioFile>("s.yaml", &text).unwrap_err();
    assert_eq!(err.pointer, "/clock/action_cost/write", "{err}");
    assert!(err.message.contains("2 parsecs"), "{err}");
}

#[test]
fn wall_is_default() {
    let text = "format: shakedown-scenario/1\nlabel: S\nworld: w\ntask: T\n";
    let scenario = parse::<ScenarioFile>("s.yaml", text).expect("parses");
    assert_eq!(scenario.clock, None);
    let clock: ClockSpec = serde_json::from_str("{}").expect("an empty clock block parses");
    assert_eq!(clock.mode, ClockMode::Wall);
    assert_eq!(clock.action_cost.read, "0m");
    assert_eq!(clock.action_cost.write, "2m");
    assert_eq!(
        clock.epoch_ms(),
        Ok(Some(3_600_000)),
        "epoch defaults to 60m"
    );
}

#[test]
fn durations_parse() {
    assert_eq!(parse_duration_ms("0m"), Ok(0));
    assert_eq!(parse_duration_ms("2m"), Ok(120_000));
    assert_eq!(parse_duration_ms("90s"), Ok(90_000));
    assert_eq!(parse_duration_ms("1h"), Ok(3_600_000));
    assert_eq!(parse_duration_ms("250ms"), Ok(250));
    assert!(parse_duration_ms("2").is_err());
    assert!(parse_duration_ms("-2m").is_err());
    let none: ClockSpec = serde_json::from_str(r#"{"epoch": "none"}"#).unwrap();
    assert_eq!(none.epoch_ms(), Ok(None));
}

#[test]
fn timed_event_needs_one_trigger() {
    let text = "format: shakedown-scenario/1\nlabel: S\nworld: w\ntask: T\nevents:\n  - {name: E, known_at: \"07:00\", after_calls: 3}\n";
    let err = parse::<ScenarioFile>("s.yaml", text).unwrap_err();
    assert_eq!(err.pointer, "/events/0", "{err}");
    let late = "format: shakedown-scenario/1\nlabel: S\nworld: w\ntask: T\nevents:\n  - {name: E, known_at: \"07:00\", part: p, do: {kind: x}}\n";
    parse::<ScenarioFile>("s.yaml", late).expect("known_at alone is a world time");
    let orphan = "format: shakedown-scenario/1\nlabel: S\nworld: w\ntask: T\nevents:\n  - {name: E, at: \"07:00\", do: {kind: x}}\n";
    let err = parse::<ScenarioFile>("s.yaml", orphan).unwrap_err();
    assert_eq!(err.pointer, "/events/0/part", "{err}");
}
