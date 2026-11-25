//! Mapping files onto the engine, on a copy of the example project.

use std::path::Path;

use shakedown_project::{Id, Project};

use crate::ops::OpError;

use super::{request as deploy_request, Credentials};
use crate::model::{AuthRef, Runner};

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy(&e.path(), &target)
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// The example project, with `edit(dir)` applied before it is opened.
fn project(edit: impl FnOnce(&Path)) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../project/tests/fixtures/larkspur"),
        dir.path(),
    );
    edit(dir.path());
    let project = Project::open(dir.path()).unwrap();
    (dir, project)
}

/// Adds `line` after the first line of `file`.
fn add_line(dir: &Path, file: &str, line: &str) {
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    let (first, rest) = text.split_once('\n').unwrap();
    std::fs::write(path, format!("{first}\n{line}\n{rest}")).unwrap();
}

fn runnable(dir: &Path) {
    add_line(dir, "worlds/larkspur-plant/world.yaml", "package: toybench");
    add_line(dir, "scenarios/supplier-slip.yaml", "preset: t1");
    let exp = dir.join("experiments/supplier-slip-solo-vs-team.yaml");
    let text = std::fs::read_to_string(&exp).unwrap().replace(
        "[solo-planner, planning-team, planner-with-rules]",
        "[solo-planner, planner-with-rules]",
    );
    std::fs::write(exp, text).unwrap();
}

/// Both names of the example project are usable.
fn creds() -> Credentials {
    let mut c = Credentials::default();
    c.provider("anthropic-main", Ok(()));
    c.credential(AuthRef::Key("planner-endpoint".into()), Ok(()));
    c
}

fn request(
    p: &Project,
    id: &Id,
    clock_start: Option<&str>,
) -> Result<crate::api::experiments::NewExperimentReq, OpError> {
    deploy_request(p, id, &creds(), clock_start)
}

fn exp() -> Id {
    Id::parse("supplier-slip-solo-vs-team").unwrap()
}

#[test]
fn maps_builtin_setup() {
    let (_dir, p) = project(runnable);
    let req = request(&p, &exp(), None).unwrap();
    assert_eq!(req.benchmark.as_deref(), Some("toybench"));
    assert_eq!(req.task, "t1");
    assert_eq!(req.runs, 5);
    assert_eq!(req.max_cost_usd, Some(1.5));
    assert_eq!(
        req.project_experiment.as_deref(),
        Some("supplier-slip-solo-vs-team")
    );
    let solo = &req.setups[0].team.agents[0];
    assert_eq!(
        (solo.model.as_deref(), solo.prompt.as_str()),
        (Some("claude-sonnet"), "You keep orders on schedule.")
    );
    assert_eq!(
        req.setups[1].team.agents[0].url.as_deref(),
        Some("https://agents.larkspur.internal/planner")
    );
}

#[test]
fn claude_code_team_from_a_file() {
    let (_dir, p) = project(|dir| {
        runnable(dir);
        std::fs::write(
            dir.join("setups/solo-planner.yaml"),
            "format: shakedown-setup/1\nlabel: Claude Code pair\nteam:\n  kind: builtin\n  agents:\n    \
             - {id: lead, kind: claude-code, role: You lead., model: opus, tools: [a]}\n    \
             - {id: helper, kind: claude-code, role: You help., model: sonnet}\n",
        )
        .unwrap();
    });
    let req = request(&p, &exp(), None).unwrap();
    let agents = &req.setups[0].team.agents;
    assert!(agents
        .iter()
        .all(|a| a.kind == crate::model::AgentKind::ClaudeCode));
    assert_eq!(
        agents
            .iter()
            .map(|a| a.model.as_deref())
            .collect::<Vec<_>>(),
        [Some("opus"), Some("sonnet")]
    );
    assert_eq!(agents[0].tools, ["a"]);
}

#[test]
fn reasoning_reaches_the_agent() {
    let (_dir, p) = project(|dir| {
        runnable(dir);
        std::fs::write(
            dir.join("setups/solo-planner.yaml"),
            "format: shakedown-setup/1\nlabel: Thinking planner\nteam:\n  kind: builtin\n  agents:\n    \
             - {id: planner, role: You plan., model: deepseek/deepseek-v4-pro, key: anthropic-main, \
             reasoning: {effort: high}, max_tokens: 32000}\n",
        )
        .unwrap();
    });
    let req = request(&p, &exp(), None).unwrap();
    let agent = &req.setups[0].team.agents[0];
    assert_eq!(agent.reasoning, Some(serde_json::json!({"effort": "high"})));
    assert_eq!(agent.max_tokens, Some(32_000));
}

#[test]
fn faults_become_disruptions() {
    let (_dir, p) = project(runnable);
    let scenario = request(&p, &exp(), None)
        .unwrap()
        .scenario
        .expect("a fault was declared");
    assert_eq!(scenario.disruptions.len(), 1);
    assert_eq!(scenario.disruptions[0].id, "pool_exhausted");
    assert_eq!(scenario.disruptions[0].params["free"], 0);
}

#[test]
fn no_package_is_explained() {
    let (_dir, p) = project(|dir| {
        runnable(dir);
        let path = dir.join("worlds/larkspur-plant/world.yaml");
        std::fs::write(
            &path,
            std::fs::read_to_string(&path)
                .unwrap()
                .replace("package: toybench\n", ""),
        )
        .unwrap();
    });
    let err = request(&p, &exp(), None).unwrap_err();
    assert_eq!(err.status.as_u16(), 422);
    assert!(
        err.message
            .contains("Larkspur Valve Works has no runtime yet"),
        "{}",
        err.message
    );
}

#[test]
fn no_preset_is_explained() {
    let (_dir, p) =
        project(|dir| add_line(dir, "worlds/larkspur-plant/world.yaml", "package: toybench"));
    let err = request(&p, &exp(), None).unwrap_err();
    assert!(err.message.contains("set preset:"), "{}", err.message);
}

#[test]
fn code_and_madl_setups_refused() {
    let (_dir, p) = project(|dir| {
        runnable(dir);
        let exp = dir.join("experiments/supplier-slip-solo-vs-team.yaml");
        std::fs::write(
            &exp,
            std::fs::read_to_string(&exp)
                .unwrap()
                .replace("[solo-planner, planner-with-rules]", "[planning-team]"),
        )
        .unwrap();
    });
    let err = request(&p, &exp(), None).unwrap_err();
    assert_eq!(err.status.as_u16(), 422);
    assert!(err.message.starts_with("planning-team:"), "{}", err.message);
}

#[test]
fn problems_block_deploy() {
    let (_dir, p) = project(|dir| {
        runnable(dir);
        let exp = dir.join("experiments/supplier-slip-solo-vs-team.yaml");
        std::fs::write(
            &exp,
            std::fs::read_to_string(&exp)
                .unwrap()
                .replace("[solo-planner, planner-with-rules]", "[solo-planner, gone]"),
        )
        .unwrap();
    });
    let err = request(&p, &exp(), None).unwrap_err();
    assert!(
        err.message
            .contains("fix supplier-slip-solo-vs-team before deploying"),
        "{}",
        err.message
    );
}

const START: Option<&str> = Some("2026-03-10T06:00:00");

/// The runnable project with a stepped clock and a storm the part `driver` carries out, known
/// half an hour before it hits.
fn stepped(dir: &Path) {
    runnable(dir);
    let path = dir.join("scenarios/supplier-slip.yaml");
    let text = std::fs::read_to_string(&path).unwrap();
    let text = text.replace(
        "events:\n",
        "clock: {mode: stepped, action_cost: {read: 0m, write: 2m, tools: {plan_recovery: 5m}}, epoch: 60m, horizon: \"23:59\"}\n\
         generators:\n  - {kind: sick_calls, rate_per_100_duties: 1.5}\n\
         events:\n  - {name: Thunderstorm cells approach the hub, at: \"15:10\", known_at: \"14:40\", part: driver, do: {kind: capacity_program, arrival_rate: 24}}\n",
    );
    std::fs::write(path, text).unwrap();
}

/// The project's timed events and generators, as the setup call of `driver` gets them.
fn driver_setup(asked: &parts::proto::ScenarioRequest) -> serde_json::Value {
    // The package's own disruptions need its options; the timed ones are given as they are.
    let timed_only = parts::proto::ScenarioRequest {
        disruptions: vec![],
        ..asked.clone()
    };
    let scenario = crate::scenario::expand(None, &timed_only).unwrap();
    let task = [parts::proto::SetupCall {
        part: "driver".into(),
        tool: "driver_scenario".into(),
        args: serde_json::json!({}),
    }];
    let calls = crate::scenario::setup_calls(&task, &scenario, None).unwrap();
    calls[0].args.clone()
}

#[test]
fn timed_events_reach_part_setup() {
    let (_dir, p) = project(stepped);
    let asked = request(&p, &exp(), START).unwrap().scenario.unwrap();
    assert_eq!(asked.timed.len(), 1, "events without a part stay faults");
    let storm = &asked.timed[0];
    assert_eq!(storm.part, "driver");
    let d = &storm.disruption;
    assert_eq!(d["id"], "thunderstorm-cells-approach-the-hub");
    assert_eq!(d["text"], "Thunderstorm cells approach the hub");
    assert_eq!(d["do"]["kind"], "capacity_program");
    assert_eq!(asked.generators[0]["kind"], "sick_calls");
    let args = driver_setup(&asked);
    assert_eq!(args["disruptions"][0], *d);
    assert_eq!(args["generators"], serde_json::json!(asked.generators));
}

#[test]
fn hhmm_resolves_on_clock_start_day() {
    let (_dir, p) = project(stepped);
    let asked = request(&p, &exp(), START).unwrap().scenario.unwrap();
    let storm = &asked.timed[0].disruption;
    assert_eq!(storm["at"], "2026-03-10T15:10:00");
    assert_eq!(storm["known_at"], "2026-03-10T14:40:00");
    assert_eq!(storm["at_ms"], (9 * 60 + 10) * 60_000);
    assert_eq!(storm["known_at_ms"], (8 * 60 + 40) * 60_000);
    assert_eq!(
        asked.clock.unwrap().horizon_ms,
        Some((17 * 60 + 59) * 60_000)
    );
    let err = request(&p, &exp(), None).unwrap_err();
    assert!(err.message.contains("no clock_start"), "{}", err.message);
}

#[test]
fn stepped_picks_dispatch() {
    let (_dir, p) = project(stepped);
    let req = request(&p, &exp(), START).unwrap();
    assert!(req.setups.iter().all(|s| s.runner == Runner::Dispatch));
    let clock = req.scenario.unwrap().clock.unwrap();
    assert_eq!((clock.read_ms, clock.write_ms), (0, 120_000));
    assert_eq!(clock.tools["plan_recovery"], 300_000);
    assert_eq!(clock.epoch_ms, Some(3_600_000));
    let (_dir, wall) = project(runnable);
    let req = request(&wall, &exp(), START).unwrap();
    assert!(req.setups.iter().all(|s| s.runner == Runner::Chain));
    assert!(req.scenario.unwrap().clock.is_none());
}

/// The stepped clock `p`'s experiment is deployed with.
fn clock_of(p: &Project) -> parts::proto::SteppedClock {
    let scenario = request(p, &exp(), START).unwrap().scenario.unwrap();
    scenario.clock.unwrap()
}

#[test]
fn team_start_is_placed_on_the_clock_start_day() {
    let (_dir, p) = project(|dir| {
        stepped(dir);
        let path = dir.join("scenarios/supplier-slip.yaml");
        let text = std::fs::read_to_string(&path).unwrap();
        let late = "horizon: \"23:59\", team_start: \"12:00\"}";
        std::fs::write(path, text.replace("horizon: \"23:59\"}", late)).unwrap();
    });
    assert_eq!(
        clock_of(&p).team_start_ms,
        Some(6 * 3_600_000),
        "12:00 on a day whose clock starts at 06:00"
    );
    let (_dir, full) = project(stepped);
    let none = clock_of(&full).team_start_ms;
    assert_eq!(none, None, "the full day stays the default");
}

#[test]
fn chain_refuses_stepped() {
    use crate::scenario::{refusal, Scenario, STEPPED_NEEDS_DISPATCH};
    let stepped = Scenario {
        clock: Some(Default::default()),
        ..Scenario::default()
    };
    for runner in [Runner::Chain, Runner::Concurrent, Runner::External] {
        assert_eq!(
            refusal(runner, &stepped).as_deref(),
            Some(STEPPED_NEEDS_DISPATCH)
        );
    }
    assert_eq!(refusal(Runner::Dispatch, &stepped), None);
    assert_eq!(refusal(Runner::Chain, &Scenario::default()), None);
    assert_eq!(
        STEPPED_NEEDS_DISPATCH,
        "stepped clock needs a team with a lead (dispatch runner)"
    );
}

#[test]
fn an_offset_on_clock_start_moves_no_scenario_time() {
    let (_dir, p) = project(stepped);
    let local = request(&p, &exp(), START).unwrap().scenario.unwrap();
    let offset = request(&p, &exp(), Some("2026-03-10T06:00:00-04:00"))
        .unwrap()
        .scenario
        .unwrap();
    let (a, b) = (&local.timed[0].disruption, &offset.timed[0].disruption);
    assert_eq!(
        (&a["at"], &a["at_ms"]),
        (&b["at"], &b["at_ms"]),
        "scenario times are world local times"
    );
    assert_eq!(
        local.clock.unwrap().horizon_ms,
        offset.clock.unwrap().horizon_ms
    );
}

#[test]
fn a_builtin_agents_key_becomes_its_provider() {
    let (_dir, p) = project(runnable);
    let req = request(&p, &exp(), None).unwrap();
    let solo = &req.setups[0].team.agents[0];
    assert_eq!(solo.provider.as_deref(), Some("key:anthropic-main"));
    assert_eq!(solo.model.as_deref(), Some("claude-sonnet"));
}

#[test]
fn a_key_that_is_not_stored_or_not_yours_is_refused() {
    let (_dir, p) = project(runnable);
    let err = deploy_request(&p, &exp(), &Credentials::default(), None).unwrap_err();
    assert_eq!(err.status.as_u16(), 400);
    assert!(
        err.message.contains("anthropic-main") && err.message.contains("solo-planner"),
        "{}",
        err.message
    );
    let mut withheld = creds();
    withheld.provider("anthropic-main", Err("is another member's".into()));
    let err = deploy_request(&p, &exp(), &withheld, None).unwrap_err();
    assert!(err.message.contains("another member's"), "{}", err.message);
}

#[test]
fn an_endpoints_auth_becomes_a_reference_without_the_secret() {
    let (_dir, p) = project(runnable);
    let req = request(&p, &exp(), None).unwrap();
    let endpoint = &req.setups[1].team.agents[0];
    assert_eq!(endpoint.auth, Some(AuthRef::Key("planner-endpoint".into())));
    assert!(endpoint.headers.is_empty());
}

#[test]
fn an_auth_that_may_not_be_used_is_refused_with_its_reason() {
    let (_dir, p) = project(runnable);
    let mut c = creds();
    c.credential(
        AuthRef::Key("planner-endpoint".into()),
        Err("the key is shared with you".into()),
    );
    let err = deploy_request(&p, &exp(), &c, None).unwrap_err();
    assert_eq!(err.status.as_u16(), 400);
    assert!(
        err.message.contains("planner-with-rules") && err.message.contains("shared with you"),
        "{}",
        err.message
    );
}

fn with_auth(auth: &'static str) -> (tempfile::TempDir, Project) {
    project(move |dir| {
        runnable(dir);
        let f = dir.join("setups/planner-with-rules.yaml");
        let text = std::fs::read_to_string(&f).unwrap();
        std::fs::write(&f, text.replace("auth: {key: planner-endpoint}", auth)).unwrap();
    })
}

#[test]
fn an_auth_naming_a_key_and_a_variable_is_refused() {
    let (_dir, p) = with_auth("auth: {key: planner-endpoint, env: SHAKEDOWN_AGENT_T}");
    let needs = super::needs(&p, &exp());
    assert_eq!(needs.err().map(|e| e.status.as_u16()), Some(400));
}

#[test]
fn an_auth_env_is_a_reference_to_the_variable() {
    let (_dir, p) = with_auth("auth: {env: SHAKEDOWN_AGENT_T}");
    let mut c = creds();
    c.credential(AuthRef::Env("SHAKEDOWN_AGENT_T".into()), Ok(()));
    let req = deploy_request(&p, &exp(), &c, None).unwrap();
    let got = &req.setups[1].team.agents[0].auth;
    assert_eq!(got, &Some(AuthRef::Env("SHAKEDOWN_AGENT_T".into())));
    let needs = super::needs(&p, &exp()).unwrap();
    assert_eq!(needs.auth, [AuthRef::Env("SHAKEDOWN_AGENT_T".into())]);
    assert_eq!(needs.providers, ["anthropic-main"]);
}

#[test]
fn problems_are_reported_before_any_key_is_looked_up() {
    let (_dir, p) = project(|dir| {
        runnable(dir);
        let exp = dir.join("experiments/supplier-slip-solo-vs-team.yaml");
        let text = std::fs::read_to_string(&exp).unwrap();
        let broken = text.replace("[solo-planner, planner-with-rules]", "[solo-planner, gone]");
        std::fs::write(&exp, broken).unwrap();
    });
    let err = super::needs(&p, &exp()).err().unwrap();
    assert!(
        err.message
            .contains("fix supplier-slip-solo-vs-team before deploying"),
        "{}",
        err.message
    );
}
