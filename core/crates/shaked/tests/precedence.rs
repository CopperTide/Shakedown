//! Which refusal a launch request gets when it is wrong in several ways at once: the status and
//! body of each, on an app whose saves are over their limit (so a 507 competes with every other
//! refusal). Without a model: nothing here starts an agent.
mod support;
use support::records;

use serde_json::{json, Value};
use shaked::model::*;
use support::{app::send, run, AppFx};

/// An app that runs no agent, its saves over their limit (the base save made).
async fn setup(tag: &str) -> AppFx {
    let fx = AppFx::idle("precedence", tag)
        .config(|c| c.max_db_bytes = 1)
        .start()
        .await;
    let bench = fx.state.bench().await.unwrap();
    shaked::world::ensure_base(&fx.state.hub, &bench)
        .await
        .unwrap();
    fx
}

/// One request: its route, raw body and extra headers (else JSON), and the refusal it gets.
struct Case {
    path: String,
    body: String,
    headers: Vec<(&'static str, &'static str)>,
    status: u16,
    error: String,
}

fn case(path: impl Into<String>, body: Value, status: u16, error: impl Into<String>) -> Case {
    Case {
        path: path.into(),
        body: body.to_string(),
        headers: vec![("content-type", "application/json")],
        status,
        error: error.into(),
    }
}

impl Case {
    fn raw(mut self, body: &str) -> Case {
        self.body = body.to_string();
        self
    }

    fn header(mut self, name: &'static str, value: &'static str) -> Case {
        self.headers.push((name, value));
        self
    }

    fn plain(mut self) -> Case {
        self.headers = vec![("content-type", "text/plain")];
        self
    }
}

/// Posts each case and asserts its status and `{"error"}` body (naming every case that differs).
async fn assert_cases(fx: &AppFx, cases: Vec<Case>) {
    let mut wrong = vec![];
    for c in cases {
        let mut req = fx.http.post(format!("{}{}", fx.base, c.path));
        for (k, v) in &c.headers {
            req = req.header(*k, *v);
        }
        let (status, body) = send(req.body(c.body.clone())).await;
        if (status, &body) != (c.status, &json!({"error": c.error})) {
            wrong.push(format!("{} {}: got {status} {body}", c.path, c.body));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

fn team(agents: Value) -> Value {
    json!({"id": "t", "name": "T", "agents": agents})
}

/// An agent shaked can run with no provider check.
fn claude() -> Value {
    json!({"id": "a", "name": "A", "kind": "claude_code", "model": "sonnet", "prompt": ""})
}

const CLEANING: &str = "Saves are being cleaned up; try again in a moment";
const OVER: &str = "Shakedown's saves use more than the limit; delete old runs";
const NO_AGENT: &str = "the team needs at least one agent";
const BAD_COST: &str = "max_cost_usd must be a positive number of dollars";

/// While saves are cleaned up, a run or fork is refused before its body is read.
#[test]
fn a_cleanup_comes_first() {
    run(async {
        let fx = setup("cleaning").await;
        let _held = fx.state.cleanup.clone().write_owned().await;
        assert_cases(
            &fx,
            vec![
                case("/api/runs", json!({}), 409, CLEANING).raw("not json"),
                case("/api/branches/nope/fork", json!({}), 409, CLEANING).raw("{"),
            ],
        )
        .await;
    })
}

/// One refusal on `path` per row: `(body, status, error)`.
fn on<E: Into<String>>(path: &str, rows: Vec<(Value, u16, E)>) -> Vec<Case> {
    let each = rows.into_iter();
    each.map(|(body, status, error)| case(path, body, status, error))
        .collect()
}

/// `POST /api/runs`: the body, then the scenario, the seed, the team, the spend limit and last
/// the disk; the task is checked only once the run starts.
fn run_cases() -> Vec<Case> {
    let (none, one) = (team(json!([])), team(json!([claude()])));
    let big = 1u64 << 53;
    let nope = json!({"disruptions": [{"id": "nope"}]});
    let not_json = "bad request body: expected ident at line 1 column 2";
    let mut cases = vec![case("/api/runs", json!({}), 400, not_json).raw("not json")];
    cases.extend(on("/api/runs", vec![
        (json!({"task_id": 5, "team": none}), 400, "bad request body: invalid type: integer `5`, expected a string at line 1 column 12"),
        (json!({"task_id": "27", "team": none, "seed": big, "scenario": nope}), 400, "the benchmark offers no disruption \"nope\""),
        (json!({"task_id": "27", "team": none, "seed": big, "max_cost_usd": -1}), 400, "seed must be at most 2^53 - 1"),
        (json!({"task_id": "27", "team": none, "max_cost_usd": -1}), 400, NO_AGENT),
        (json!({"task_id": "nope", "team": one, "max_cost_usd": -1}), 400, BAD_COST),
        (json!({"task_id": "nope", "team": one}), 507, OVER),
    ]));
    cases
}

/// `POST /api/runs` with `runner: external`: no spend limit is read, and only the agents are
/// checked before the disk.
fn external_run_cases() -> Vec<Case> {
    let bad = json!({"id": "Bad Id", "name": "B", "kind": "external", "model": null, "prompt": ""});
    let run = |agents: Value| json!({"task_id": "nope", "team": team(agents), "runner": "external", "max_cost_usd": -1});
    on(
        "/api/runs",
        vec![
            (
                run(json!([bad])),
                400,
                "agent id \"Bad Id\" must match ^[a-z0-9_-]+$",
            ),
            (run(json!([])), 507, OVER),
        ],
    )
}

/// `POST /api/runs` with a Code connection: the spend limit before the launch guard (403, 415),
/// then the runner, then the connection.
fn code_run_cases() -> Vec<Case> {
    let run = |extra: Value| {
        let mut body = json!({"task_id": "27", "team": team(json!([])), "connection": "nope"});
        let extra = extra.as_object().unwrap().clone();
        body.as_object_mut().unwrap().extend(extra);
        body
    };
    let (costly, dispatch) = (
        run(json!({"max_cost_usd": -1})),
        run(json!({"runner": "dispatch"})),
    );
    let evil = "a request from http://evil.test can't start a driver on this machine";
    let plain = "a request that starts a driver must be JSON (content-type: application/json)";
    let driven = "a run with a Code connection is driven by its driver";
    vec![
        case("/api/runs", costly, 400, BAD_COST).header("origin", "http://evil.test"),
        case("/api/runs", dispatch.clone(), 403, evil).header("origin", "http://evil.test"),
        case("/api/runs", dispatch.clone(), 415, plain).plain(),
        case("/api/runs", dispatch, 400, driven),
        case("/api/runs", run(json!({})), 404, "no connection \"nope\""),
    ]
}

#[test]
fn a_new_run_is_refused_in_this_order() {
    run(async {
        let fx = setup("runs").await;
        assert_cases(&fx, run_cases()).await;
        assert_cases(&fx, external_run_cases()).await;
        assert_cases(&fx, code_run_cases()).await;
        assert!(records(&fx.state.store).runs().await.unwrap().is_empty());
    })
}

/// A finished root branch of task 27.
async fn finished_branch(fx: &AppFx) -> String {
    let t = Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![AgentSpec::new("a", AgentKind::ClaudeCode)],
    };
    let run_id = records(&fx.state.store).create_run("27", &t).await.unwrap();
    let store = &fx.state.store;
    let none = json!([]);
    let b = records(store)
        .create_branch(&run_id, None, &t, &none, None)
        .await;
    let b = b.unwrap();
    records(store)
        .set_status(&b, BranchStatus::Passed, None)
        .await
        .unwrap();
    b
}

/// A fork: the body, then the branch (404, 409 while it runs), the team, and last the disk.
#[test]
fn a_fork_is_refused_in_this_order() {
    run(async {
        let fx = setup("fork").await;
        let b = finished_branch(&fx).await;
        let fork = format!("/api/branches/{b}/fork");
        let none = team(json!([]));
        assert_cases(
            &fx,
            vec![
                case(
                    "/api/branches/nope/fork",
                    json!({}),
                    400,
                    "bad request body: missing field `at_seq` at line 1 column 2",
                ),
                case(
                    "/api/branches/nope/fork",
                    json!({"at_seq": 1, "team": none}),
                    404,
                    "there is no branch \"nope\"",
                ),
                case(&fork, json!({"at_seq": 1, "team": none}), 400, NO_AGENT),
                case(&fork, json!({"at_seq": 1}), 507, OVER),
            ],
        )
        .await;
    })
}

const GONE: &str = "no live branch \"nope\"";
const BAD_ID: &str = "agent id \"Bad Id\" must match ^[a-z0-9_-]+$";

/// `bad request body: invalid type: {what} at line 1 column {col}`.
fn bad_type(what: &str, col: u32) -> String {
    format!("bad request body: invalid type: {what} at line 1 column {col}")
}

/// `POST /api/branches/nope/{what}`.
fn at(what: &str) -> String {
    format!("/api/branches/nope/{what}")
}

/// The routes an external system drives a run with: the body (and what it says) before the
/// branch, the branch before the agent.
fn external_cases() -> Vec<Case> {
    let int = |n: u32| bad_type("integer `5`, expected a string", n);
    let mut cases = on(
        &at("agents"),
        vec![
            (json!({"id": 5}), 400, int(7)),
            (json!({"id": "Bad Id"}), 400, BAD_ID.into()),
            (json!({"id": "a"}), 404, GONE.into()),
        ],
    );
    let few = "a reasoning report carries 1 to 100 items";
    cases.push(case(
        at("agents/a/reasoning"),
        json!({"items": []}),
        400,
        few,
    ));
    let i64 = bad_type("string \"x\", expected i64", 19);
    cases.push(case(at("agents/a/usage"), json!({"x": 1}), 404, GONE));
    cases.push(case(
        at("agents/a/usage"),
        json!({"input_tokens": "x"}),
        400,
        i64,
    ));
    cases
}

/// The end of an external run's agent, and the run's finish: an empty body is no body.
fn end_cases() -> Vec<Case> {
    let boolean = bad_type("string \"x\", expected a boolean", 9);
    vec![
        case(at("agents/a/end"), json!({"ok": "x"}), 400, boolean),
        case(at("agents/a/end"), json!({}), 404, GONE).raw(""),
        case(
            at("finish"),
            json!({"error": 5}),
            400,
            bad_type("integer `5`, expected a string", 10),
        ),
        case(at("finish"), json!({}), 404, GONE).raw(""),
    ]
}

#[test]
fn an_external_call_is_refused_in_this_order() {
    run(async {
        let fx = setup("external").await;
        assert_cases(&fx, external_cases()).await;
        assert_cases(&fx, end_cases()).await;
    })
}

/// `POST /api/suites`: the body, the repeats and team, the tasks, then the disk, and only then the
/// spend limit. `POST /api/experiments`: the setups, the spend limit, the task, then the disk.
fn suite_cases() -> Vec<Case> {
    let one = team(json!([claude()]));
    let setups = json!([{"team": one}]);
    let no_task = "the benchmark has no task \"nope\"";
    let mut cases = on("/api/suites", vec![
        (json!({"team": 1}), 400, "bad request body: invalid type: integer `1`, expected struct Team at line 1 column 9"),
        (json!({"team": team(json!([])), "repeats": 0, "tasks": ["nope"]}), 400, "repeats must be from 1 to 100"),
        (json!({"team": one, "tasks": ["nope"], "max_cost_usd": -1}), 400, no_task),
        (json!({"team": one, "max_cost_usd": -1}), 507, OVER),
    ]);
    cases.extend(on(
        "/api/experiments",
        vec![
            (
                json!({"task": "nope", "setups": setups, "max_cost_usd": -1}),
                400,
                BAD_COST,
            ),
            (json!({"task": "nope", "setups": setups}), 400, no_task),
            (json!({"task": "27", "setups": setups}), 507, OVER),
        ],
    ));
    cases
}

#[test]
fn a_suite_or_experiment_is_refused_in_this_order() {
    run(async {
        let fx = setup("suites").await;
        assert_cases(&fx, suite_cases()).await;
        assert!(records(&fx.state.store).suites().await.unwrap().is_empty());
    })
}

/// With room on the disk, a run of a task the benchmark lacks is 404 once it starts, built-in or
/// external.
#[test]
fn an_unknown_task_is_404_once_the_run_starts() {
    run(async {
        let fx = AppFx::idle("precedence", "task").start().await;
        let one = team(json!([claude()]));
        let none = team(json!([]));
        let external = json!({"task_id": "nope", "team": none, "runner": "external"});
        let rows = vec![
            (
                json!({"task_id": "nope", "team": one}),
                404,
                "unknown task nope",
            ),
            (external, 404, "unknown task nope"),
        ];
        assert_cases(&fx, on("/api/runs", rows)).await;
    })
}
