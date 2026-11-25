//! The control box's end of the worker link (`GET /worker`): a test client speaks the frames a
//! `shaked worker` in a sandbox would. The branch is made by `record_start`, so it has no events
//! before the worker's first (the worker records `RunStart` itself).
mod support;
use support::link::{self, hear, heard, hello, say, ticket_for, Ws};
use support::{app::send, pg::query_text, poll_until, records, run, team_of, AppFx, STEP};

use std::{sync::Arc, time::Duration};

use serde_json::{json, Value};
use shaked::{
    app::Config,
    auth::{run_tokens::resolve_run_token, Decision},
    model::*,
    runner::BranchPlan,
    worker::frames::{ToControl, ToWorker},
};

async fn setup(tag: &str) -> AppFx {
    AppFx::idle("worker", tag).bench("retail").start().await
}

/// `setup` with the link's hello and idle timeouts at `ms` milliseconds.
async fn impatient(tag: &str, ms: u64) -> AppFx {
    let fx = AppFx::idle("worker", tag).bench("retail");
    let fx = fx.config(move |c| {
        c.link_hello_timeout = Duration::from_millis(ms);
        c.link_idle_timeout = Duration::from_millis(ms);
    });
    fx.start().await
}

/// A branch made by `record_start`, expected on the link, and a ticket for it.
async fn branch(fx: &AppFx) -> (BranchPlan, String) {
    let team = team_of(vec![
        AgentSpec::new("a", AgentKind::ClaudeCode).with_model("sonnet")
    ]);
    link::ticketed(fx, "27", team, Rules::new(Runner::Chain)).await
}

/// True when the control box sends no frame (pings aside) within `ms` milliseconds.
async fn quiet_for(ws: &mut Ws, ms: u64) -> bool {
    tokio::time::timeout(Duration::from_millis(ms), heard(ws))
        .await
        .is_err()
}

fn event(n: u64) -> ToControl {
    ToControl::Event {
        n,
        agent: Some("a".into()),
        kind: EventKind::Message,
        payload: json!({"text": format!("event {n}")}),
        overhead: None,
    }
}

/// Says hello with `ticket`: the link and the worker token `Start` carried.
async fn started(fx: &AppFx, plan: &BranchPlan, ticket: &str) -> (Ws, String) {
    let mut ws = link::joined(&fx.base, ticket, 0).await;
    let ToWorker::Start {
        plan: got,
        token,
        gateway,
        ..
    } = hear(&mut ws).await
    else {
        panic!("the first answer is start");
    };
    assert_eq!(*got, *plan);
    assert!(token.starts_with("sdw_"), "{token}");
    assert_eq!(gateway, format!("{}/gateway", fx.base));
    (ws, token)
}

/// The branch's events as `(seq, text)`.
async fn texts(fx: &AppFx, branch: &str) -> Vec<(i64, String)> {
    let evs = records(&fx.state.store).events(branch).await.unwrap();
    let text = |p: &Value| p["text"].as_str().unwrap_or_default().to_string();
    evs.iter().map(|e| (e.seq, text(&e.payload))).collect()
}

fn numbered(ns: std::ops::RangeInclusive<i64>) -> Vec<(i64, String)> {
    ns.map(|n| (n, format!("event {n}"))).collect()
}

async fn refused(ws: &mut Ws) -> String {
    match hear(ws).await {
        ToWorker::Refused { why } => why,
        other => panic!("refused, not {other:?}"),
    }
}

#[test]
fn a_worker_records_its_events_in_order() {
    run(async {
        let fx = setup("order").await;
        let (plan, ticket) = branch(&fx).await;
        let id = plan.branch_id.clone();
        assert!(
            texts(&fx, &id).await.is_empty(),
            "nothing before the worker"
        );
        let (mut ws, token) = started(&fx, &plan, &ticket).await;
        assert!(fx.state.links.connected(&id));
        for n in 1..=3 {
            say(&mut ws, &event(n)).await;
        }
        for n in 1..=3 {
            let seq = n as i64;
            assert_eq!(hear(&mut ws).await, ToWorker::Ack { n, seq });
        }
        assert_eq!(texts(&fx, &id).await, numbered(1..=3));
        let ended = ToControl::Ended {
            status: BranchStatus::Passed,
            error: None,
        };
        say(&mut ws, &ended).await;
        assert_eq!(heard(&mut ws).await, None, "the link closes after ended");
        let b = records(&fx.state.store).find_branch(&id).await.unwrap();
        assert_eq!(b.unwrap().status, BranchStatus::Passed);
        let left = resolve_run_token(&fx.state.store, &token).await.unwrap();
        assert!(left.is_none(), "the worker token dies with its branch");
    });
}

/// `POST` (or `GET` without a body) to `path` with `token` as the Bearer: the status.
async fn as_token(fx: &AppFx, token: &str, post: bool, path: &str) -> u16 {
    let url = format!("{}{path}", fx.base);
    let req = match post {
        true => fx.http.post(url).json(&json!({})),
        false => fx.http.get(url),
    };
    send(req.bearer_auth(token)).await.0
}

#[test]
fn a_ticket_works_once_and_only_for_its_branch() {
    run(async {
        let fx = setup("ticket").await;
        let (a, ticket) = branch(&fx).await;
        let (b, _) = branch(&fx).await;
        let (_link, token) = started(&fx, &a, &ticket).await;

        let mut again = link::joined(&fx.base, &ticket, 0).await;
        assert!(refused(&mut again).await.contains("ticket"));

        let expired = (fx.state.links.tickets).issue_lasting(ticket_for(&b), Duration::ZERO);
        let mut late = link::joined(&fx.base, &expired, 0).await;
        assert!(refused(&mut late).await.contains("ticket"));
        assert!(!fx.state.links.connected(&b.branch_id));

        // A's worker token writes nothing of B's, and reaches no API route, A's included.
        let (ab, bb) = (&a.branch_id, &b.branch_id);
        let on_b = format!("/api/branches/{bb}/sandbox-events");
        assert_eq!(as_token(&fx, &token, true, &on_b).await, 403);
        assert_eq!(
            as_token(&fx, &token, true, &format!("/api/branches/{bb}/finish")).await,
            403
        );
        assert_eq!(
            as_token(&fx, &token, false, &format!("/api/branches/{ab}")).await,
            403
        );
        assert_eq!(as_token(&fx, &token, false, "/api/runs").await, 403);
        assert!(texts(&fx, bb).await.is_empty());

        let mut forged = link::joined(&fx.base, "sdw_0000", 0).await;
        assert!(refused(&mut forged).await.contains("token"));
    });
}

#[test]
fn a_dropped_connection_loses_and_doubles_nothing() {
    run(async {
        let fx = setup("restart").await;
        let (plan, ticket) = branch(&fx).await;
        let id = plan.branch_id.clone();
        let (mut ws, token) = started(&fx, &plan, &ticket).await;
        for n in 1..=5 {
            say(&mut ws, &event(n)).await;
        }
        // All five are appended and acked; the worker is taken to have lost the acks of 4 and 5.
        for n in 1..=5 {
            assert!(matches!(hear(&mut ws).await, ToWorker::Ack { n: m, .. } if m == n));
        }
        drop(ws);

        let mut ws = link::joined(&fx.base, &token, 3).await;
        for n in 4..=6 {
            say(&mut ws, &event(n)).await;
        }
        for n in 4..=6 {
            let seq = n as i64;
            assert_eq!(hear(&mut ws).await, ToWorker::Ack { n, seq });
        }
        assert_eq!(texts(&fx, &id).await, numbered(1..=6));

        let mut ahead = link::joined(&fx.base, &token, 9).await;
        assert!(refused(&mut ahead).await.contains("resume_from"));
    });
}

#[test]
fn pause_from_the_ui_reaches_the_worker() {
    run(async {
        let fx = setup("pause").await;
        let (plan, ticket) = branch(&fx).await;
        let id = plan.branch_id.clone();
        let (mut ws, _) = started(&fx, &plan, &ticket).await;
        let on = |what: &str| format!("/api/branches/{id}/{what}");
        let (status, v) = fx.post(&on("pause"), json!({})).await;
        assert_eq!((status, v), (200, json!({"paused": true, "changed": true})));
        assert_eq!(hear(&mut ws).await, ToWorker::Pause);
        let (status, v) = fx.post(&on("resume"), json!({})).await;
        assert_eq!(
            (status, v),
            (200, json!({"paused": false, "changed": true}))
        );
        assert_eq!(hear(&mut ws).await, ToWorker::Resume);
        let (status, v) = fx.post(&on("stop"), json!({})).await;
        assert_eq!((status, v), (200, json!({"stopping": true})));
        assert_eq!(hear(&mut ws).await, ToWorker::Stop);
    });
}

/// Sends events `ns` and checks each is acked with the seq paired with it.
async fn acked(ws: &mut Ws, ns: &[(u64, i64)]) {
    for (n, _) in ns {
        say(ws, &event(*n)).await;
    }
    for &(n, seq) in ns {
        assert_eq!(hear(ws).await, ToWorker::Ack { n, seq });
    }
}

#[test]
fn a_restarted_control_box_rejoins_without_doubling() {
    run(async {
        let fx = setup("rejoin").await;
        let (plan, ticket) = branch(&fx).await;
        let id = plan.branch_id.clone();
        let (mut ws, token) = started(&fx, &plan, &ticket).await;
        acked(&mut ws, &[(1, 1), (2, 2), (3, 3)]).await;
        // The control box records one of its own: the worker's n and the seqs part.
        let store = records(&fx.state.store);
        let own = store.append(&id, None, EventKind::WorldEvent, json!({}));
        assert_eq!(own.await.unwrap().seq, 4);
        acked(&mut ws, &[(4, 5), (5, 6)]).await;
        drop(ws);

        let (again, base) = link::restarted(&fx, Config::new()).await;
        assert!(!again.links.connected(&id));
        let mut ws = link::joined(&base, &token, 3).await;
        acked(&mut ws, &[(4, 5), (5, 6), (6, 7)]).await;
        assert!(again.links.connected(&id));
        let mut want = numbered(1..=7);
        want[3].1 = String::new(); // seq 4 is the control box's own
        for (seq, text) in &mut want[4..] {
            *text = format!("event {}", *seq - 1);
        }
        assert_eq!(texts(&fx, &id).await, want, "each event once, in order");
    });
}

#[test]
fn two_tickets_for_one_branch_start_one_worker() {
    run(async {
        let fx = setup("race").await;
        let (plan, first) = branch(&fx).await;
        let second = fx.state.links.tickets.issue(ticket_for(&plan));
        let (mut a, mut b) = (link::connect(&fx.base).await, link::connect(&fx.base).await);
        say(&mut a, &hello(&first, 0)).await;
        say(&mut b, &hello(&second, 0)).await;
        let answers = [hear(&mut a).await, hear(&mut b).await];
        let starts = answers
            .iter()
            .filter(|f| matches!(f, ToWorker::Start { .. }));
        let refusals = answers
            .iter()
            .filter(|f| matches!(f, ToWorker::Refused { .. }));
        assert_eq!((starts.count(), refusals.count()), (1, 1), "{answers:?}");
        let live = format!(
            "SELECT count(*) FROM shakedown.run_tokens WHERE branch_id = '{}'
             AND kind = 'worker' AND revoked_at IS NULL",
            plan.branch_id
        );
        assert_eq!(query_text(&fx.db, &live).await.as_deref(), Some("1"));
    });
}

#[test]
fn a_link_without_hello_in_time_is_refused() {
    run(async {
        let fx = impatient("hello", 300).await;
        let mut ws = link::connect(&fx.base).await;
        assert!(refused(&mut ws).await.contains("hello"));
        assert_eq!(heard(&mut ws).await, None, "and closed");
    });
}

#[test]
fn a_silent_link_is_closed() {
    run(async {
        let fx = impatient("idle", 300).await;
        let (plan, ticket) = branch(&fx).await;
        let (_ws, _) = started(&fx, &plan, &ticket).await;
        // The client never reads again, so it answers no ping.
        let gone = async || (!fx.state.links.connected(&plan.branch_id)).then_some(());
        let within = poll_until(Duration::from_secs(15), STEP, gone).await;
        assert!(within.is_some(), "connected() went false");
    });
}

/// A pause pressed before the branch's worker started is kept, and the worker hears it right
/// after its `Start`.
#[test]
fn a_pause_before_the_worker_starts_is_told_it_after_start() {
    run(async {
        let fx = setup("unlinked").await;
        let (plan, ticket) = branch(&fx).await;
        let path = format!("/api/branches/{}/pause", plan.branch_id);
        let (status, v) = fx.post(&path, json!({})).await;
        assert_eq!((status, v), (200, json!({"paused": true, "changed": true})));
        let (mut ws, _) = started(&fx, &plan, &ticket).await;
        assert_eq!(hear(&mut ws).await, ToWorker::Pause);
    });
}

#[test]
fn a_caller_who_cannot_see_the_branch_relays_nothing() {
    run(async {
        let hidden = shaked::harness::SpyLayer::deny(Decision::NotFound);
        let fx = AppFx::idle("worker", "hidden").bench("retail");
        let fx = fx.layer(Arc::new(hidden)).start().await;
        let (plan, ticket) = branch(&fx).await;
        let (mut ws, _) = started(&fx, &plan, &ticket).await;
        let path = format!("/api/branches/{}/pause", plan.branch_id);
        assert_eq!(fx.post(&path, json!({})).await.0, 404);
        assert!(quiet_for(&mut ws, 300).await, "no pause reached the worker");
    });
}

/// Puts `plan`'s run into a new experiment (through a suite of it): the experiment's id.
async fn in_experiment(fx: &AppFx, plan: &BranchPlan, tag: &str) -> String {
    let (exp, suite) = (format!("exp-{tag}"), format!("suite-{tag}"));
    let sql = format!(
        "INSERT INTO shakedown.experiments (id, name, task_id, benchmark, runs)
           VALUES ('{exp}', 'e', '27', 'retail', 1);
         INSERT INTO shakedown.suites (id, name, team, tasks, repeats, runner, benchmark, experiment_id)
           VALUES ('{suite}', 's', '{{}}', '[]', 1, 'chain', 'retail', '{exp}');
         INSERT INTO shakedown.suite_runs (suite_id, ord, task_id, repeat, run_id, branch_id)
           VALUES ('{suite}', 0, '27', 0, '{}', '{}');",
        plan.run_id, plan.branch_id
    );
    support::pg::execute(&fx.db, &sql).await;
    exp
}

async fn ended_at(fx: &AppFx, experiment: &str) -> Option<String> {
    let sql = format!("SELECT ended_at FROM shakedown.experiments WHERE id = '{experiment}'");
    query_text(&fx.db, &sql).await
}

async fn status(fx: &AppFx, branch: &str) -> BranchStatus {
    let b = records(&fx.state.store).find_branch(branch).await.unwrap();
    b.unwrap().status
}

#[test]
fn a_real_restart_keeps_a_linked_branch_running() {
    run(async {
        let fx = setup("startup").await;
        let (plan, ticket) = branch(&fx).await;
        let (idle, _) = branch(&fx).await;
        let (linked_exp, idle_exp) = (
            in_experiment(&fx, &plan, "a").await,
            in_experiment(&fx, &idle, "b").await,
        );
        let (mut ws, token) = started(&fx, &plan, &ticket).await;
        acked(&mut ws, &[(1, 1), (2, 2)]).await;
        drop(ws);

        // What a restarted shaked does first.
        shaked::jobs::interrupt_at_start(&fx.state.store)
            .await
            .unwrap();
        assert_eq!(status(&fx, &plan.branch_id).await, BranchStatus::Running);
        assert_eq!(ended_at(&fx, &linked_exp).await, None);
        assert_eq!(
            status(&fx, &idle.branch_id).await,
            BranchStatus::Interrupted
        );
        assert!(
            ended_at(&fx, &idle_exp).await.is_some(),
            "no worker: ended as before"
        );

        let (_again, base) = link::restarted(&fx, Config::new()).await;
        let mut ws = link::joined(&base, &token, 1).await;
        acked(&mut ws, &[(2, 2), (3, 3)]).await;
        assert_eq!(texts(&fx, &plan.branch_id).await, numbered(1..=3));
    });
}

#[test]
fn a_worker_token_of_an_ended_branch_cannot_rejoin() {
    run(async {
        let fx = setup("endedrejoin").await;
        let (plan, ticket) = branch(&fx).await;
        let (ws, token) = started(&fx, &plan, &ticket).await;
        drop(ws);
        let store = records(&fx.state.store);
        let ended = store.set_status(&plan.branch_id, BranchStatus::Interrupted, None);
        ended.await.unwrap();
        for base in [fx.base.clone(), link::restarted(&fx, Config::new()).await.1] {
            let mut ws = link::joined(&base, &token, 0).await;
            assert!(refused(&mut ws).await.contains("ended"), "{base}");
        }
        assert!(texts(&fx, &plan.branch_id).await.is_empty());
    });
}

#[test]
fn a_quiet_worker_that_answers_pings_stays_linked() {
    run(async {
        let fx = impatient("pings", 300).await;
        let (plan, ticket) = branch(&fx).await;
        let (mut ws, _) = started(&fx, &plan, &ticket).await;
        // Reading answers the control box's pings; no frame comes for three idle timeouts.
        assert!(quiet_for(&mut ws, 1000).await);
        assert!(fx.state.links.connected(&plan.branch_id));
    });
}
