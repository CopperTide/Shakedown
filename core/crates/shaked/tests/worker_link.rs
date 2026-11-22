//! The control box's end of the worker link (`GET /worker`): a test client speaks the frames a
//! `shaked worker` in a sandbox would. The branch is made by `record_start`, so it has no events
//! before the worker's first (the worker records `RunStart` itself).
mod support;
use support::{app::send, pg::query_text, records, run, AppFx};

use std::{sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use shaked::{
    app::{AppState, Config},
    auth::{run_tokens::resolve_run_token, Caller, Decision},
    launch::LaunchSpec,
    model::*,
    runner::{self, BranchPlan},
    scenario::Scenario,
    worker::{
        frames::{ToControl, ToWorker},
        Ticket,
    },
};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

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

fn team() -> Team {
    Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![AgentSpec::new("a", AgentKind::ClaudeCode).with_model("sonnet")],
    }
}

fn ticket_for(plan: &BranchPlan) -> Ticket {
    Ticket {
        branch_id: plan.branch_id.clone(),
        sandbox: Uuid::new_v4(),
        caller: Arc::new(Caller::person()),
    }
}

/// A branch made by `record_start`, expected on the link, and a ticket for it.
async fn branch(fx: &AppFx) -> (BranchPlan, String) {
    let scope = Arc::new(Caller::person());
    let scenario = Scenario::default();
    let spec = LaunchSpec {
        scope: scope.clone(),
        task_id: "27",
        team: team(),
        scenario: &scenario,
        rules: Rules::new(Runner::Chain),
        driver: None,
        benchmark: None,
    };
    let plan = runner::record_start(&fx.state, &scope, &spec)
        .await
        .unwrap();
    fx.state.links.expect(plan.clone());
    let ticket = fx.state.links.tickets.issue(ticket_for(&plan));
    (plan, ticket)
}

async fn connect(fx: &AppFx) -> Ws {
    connect_to(&fx.base).await
}

async fn connect_to(base: &str) -> Ws {
    let url = format!("{}/worker", base.replace("http://", "ws://"));
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

async fn say(ws: &mut Ws, f: &ToControl) {
    let text = serde_json::to_string(f).unwrap();
    ws.send(Message::text(text)).await.unwrap();
}

/// The next frame the control box sends; None once it has closed the link.
async fn heard(ws: &mut Ws) -> Option<ToWorker> {
    loop {
        let next = tokio::time::timeout(Duration::from_secs(10), ws.next()).await;
        match next.expect("the control box answers within 10 s") {
            Some(Ok(Message::Text(t))) => return Some(serde_json::from_str(&t).unwrap()),
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            _ => return None,
        }
    }
}

async fn hear(ws: &mut Ws) -> ToWorker {
    heard(ws).await.expect("a frame, not a closed link")
}

/// True when the control box sends no frame (pings aside) within `ms` milliseconds.
async fn quiet_for(ws: &mut Ws, ms: u64) -> bool {
    tokio::time::timeout(Duration::from_millis(ms), heard(ws))
        .await
        .is_err()
}

fn hello(ticket: &str, resume_from: u64) -> ToControl {
    ToControl::Hello {
        ticket: ticket.into(),
        resume_from,
    }
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
    let mut ws = connect(fx).await;
    say(&mut ws, &hello(ticket, 0)).await;
    let ToWorker::Start {
        plan: got,
        token,
        gateway,
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

        let mut again = connect(&fx).await;
        say(&mut again, &hello(&ticket, 0)).await;
        assert!(refused(&mut again).await.contains("ticket"));

        let expired = (fx.state.links.tickets).issue_lasting(ticket_for(&b), Duration::ZERO);
        let mut late = connect(&fx).await;
        say(&mut late, &hello(&expired, 0)).await;
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

        let mut forged = connect(&fx).await;
        say(&mut forged, &hello("sdw_0000", 0)).await;
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

        let mut ws = connect(&fx).await;
        say(&mut ws, &hello(&token, 3)).await;
        for n in 4..=6 {
            say(&mut ws, &event(n)).await;
        }
        for n in 4..=6 {
            let seq = n as i64;
            assert_eq!(hear(&mut ws).await, ToWorker::Ack { n, seq });
        }
        assert_eq!(texts(&fx, &id).await, numbered(1..=6));

        let mut ahead = connect(&fx).await;
        say(&mut ahead, &hello(&token, 9)).await;
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

/// A second control box on `fx`'s database, with nothing in memory: its base URL.
async fn restarted(fx: &AppFx) -> (Arc<AppState>, String) {
    let (listener, base) = support::app::bind().await;
    let mut config = Config::new();
    config.self_base = base.clone();
    let store = fx.state.store.clone();
    let state = Arc::new(AppState::new(store, fx.state.hub.clone(), config));
    support::app::serve(listener, &state);
    (state, base)
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

        let (again, base) = restarted(&fx).await;
        assert!(!again.links.connected(&id));
        let mut ws = connect_to(&base).await;
        say(&mut ws, &hello(&token, 3)).await;
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
        let (mut a, mut b) = (connect(&fx).await, connect(&fx).await);
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
        let mut ws = connect(&fx).await;
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
        let gone = async {
            while fx.state.links.connected(&plan.branch_id) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        let within = tokio::time::timeout(Duration::from_secs(5), gone).await;
        assert!(within.is_ok(), "connected() went false");
    });
}

#[test]
fn a_pause_with_no_worker_connected_is_a_conflict() {
    run(async {
        let fx = setup("unlinked").await;
        let (plan, _) = branch(&fx).await;
        let path = format!("/api/branches/{}/pause", plan.branch_id);
        assert_eq!(fx.post(&path, json!({})).await.0, 409);
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
