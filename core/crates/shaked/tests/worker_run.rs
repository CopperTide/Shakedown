//! `shaked worker` end to end. A test control box (no `Hosting`) makes a branch with
//! `record_start` and issues a ticket for it; a `shaked worker` child process takes the ticket
//! from a file, connects to the control box's `/worker`, stands the toy world up and drives one
//! scripted agent (an `http` agent written here, no model), recording every event through the
//! link. The branch ends on the control box as a local run of the same team ends.
mod support;
use support::{app, live, packages, records, run, AppFx};

use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};

use axum::{extract::State, routing::post, Json, Router};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use shaked::{
    auth::Caller,
    exec::BranchRecords,
    launch::LaunchSpec,
    model::*,
    runner::{self, BranchPlan},
    scenario::Scenario,
    worker::{
        link::MAX_LINK_FRAME,
        link_records::KEPT_STRING,
        run::{open, Patience},
        Ticket,
    },
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::broadcast,
};
use uuid::Uuid;

/// What the scripted agent does between listing its tools and calling one.
type Between = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

/// The scripted agent doing nothing between its calls.
fn nothing() -> Between {
    Arc::new(|| Box::pin(async {}))
}

/// An MCP call's answer; Null when the endpoint is gone (its branch was stopped).
async fn mcp(url: &str, token: &str, body: Value) -> Value {
    let req = reqwest::Client::new().post(url).bearer_auth(token);
    match req.json(&body).send().await {
        Ok(r) => r.json().await.unwrap_or(Value::Null),
        Err(_) => Value::Null,
    }
}

/// The scripted agent: lists its tools, pokes the box once and answers done.
async fn work(State(between): State<Between>, Json(run): Json<Value>) -> Json<Value> {
    if run["type"] == "hello" {
        return Json(json!({}));
    }
    let (url, token) = (
        run["mcp_url"].as_str().unwrap(),
        run["token"].as_str().unwrap(),
    );
    let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    mcp(url, token, list).await;
    between().await;
    let call = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                      "params": {"name": "poke", "arguments": {}}});
    mcp(url, token, call).await;
    Json(json!({"ok": true}))
}

/// The scripted agent served on a free port: the team of it alone.
async fn scripted_team(between: Between) -> Team {
    let app = Router::new().route("/run", post(work)).with_state(between);
    let (listener, base) = app::bind().await;
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let agent = json!({"id": "a", "name": "A", "kind": "http", "url": format!("{base}/run")});
    serde_json::from_value(json!({"id": "t", "name": "T", "agents": [agent]})).unwrap()
}

/// A control box on the scored toy world (task `t1`, solved by one poke), and the folder of
/// the toy packages.
async fn control(tag: &str) -> (AppFx, PathBuf) {
    let root = packages::toy_world(&format!("wrun-{tag}"), &[], &["--scored"]);
    let dir = root.0.clone();
    let fx = AppFx::idle("worker_run", tag).root(root).bench("toybench");
    (fx.start().await, dir)
}

/// A branch made by `record_start` for `team`, expected on the link, and a ticket for it.
async fn branch(fx: &AppFx, team: Team) -> (BranchPlan, String) {
    let scope = Arc::new(Caller::person());
    let scenario = Scenario::default();
    let spec = LaunchSpec {
        scope: scope.clone(),
        task_id: "t1",
        team,
        scenario: &scenario,
        rules: Rules::new(Runner::Chain),
        driver: None,
        benchmark: None,
    };
    let plan = runner::record_start(&fx.state, &scope, &spec).await;
    let plan = plan.unwrap();
    fx.state.links.expect(plan.clone());
    let ticket = fx.state.links.tickets.issue(Ticket {
        branch_id: plan.branch_id.clone(),
        sandbox: Uuid::new_v4(),
        caller: scope,
    });
    (plan, ticket)
}

/// A `shaked worker` child process, killed when dropped.
struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Worker {
    /// Starts `shaked worker` on `link`, the toy packages in `toys`, its ticket to come in
    /// `ticket_file`, with more environment `env`; its saves and worlds go under `scratch`.
    fn start(
        link: &str,
        ticket_file: &Path,
        toys: &Path,
        scratch: &Path,
        env: &[(&str, &str)],
    ) -> Worker {
        let file = ticket_file.to_str().unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_shaked"))
            .args(["worker", "--link", link, "--ticket-file", file])
            // A worker holds no database; the guard keeps a shaked that would serve anyway (an
            // old binary without `worker`) off the shared default one.
            .env("SHAKEDOWN_TEST_GUARD", "1")
            .env("SHAKEDOWN_PACKAGES", toys)
            .env("SHAKEDOWN_SAVES", scratch.join("saves"))
            .env("SHAKEDOWN_WORLDS", scratch.join("worlds"))
            .envs(env.iter().copied())
            .spawn()
            .unwrap();
        Worker(child)
    }

    /// Its exit code, once it exits (within a minute).
    async fn exit_code(&mut self) -> Option<i32> {
        for _ in 0..600 {
            if let Some(st) = self.0.try_wait().unwrap() {
                return st.code();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("the worker did not exit within a minute");
    }
}

fn ws_url(base: &str) -> String {
    format!("{}/worker", base.replace("http://", "ws://"))
}

/// The kinds of `branch`'s events on the control box, and their seqs.
async fn recorded(fx: &AppFx, branch: &str) -> (Vec<EventKind>, Vec<i64>) {
    let evs = records(&fx.state.store).events(branch).await.unwrap();
    let kinds = evs.iter().map(|e| e.kind).collect();
    (kinds, evs.iter().map(|e| e.seq).collect())
}

/// The kinds a local run of `team` on the control box records, once it has ended `passed`.
async fn local_kinds(fx: &AppFx, team: &Team) -> Vec<EventKind> {
    let body = json!({"task_id": "t1", "team": team, "runner": "chain"});
    let (status, v) = fx.post("/api/runs", body).await;
    assert_eq!(status, 200, "{v}");
    let id = v["branch_id"].as_str().unwrap();
    let b = live::ended(&fx.state, id).await;
    assert_eq!(b.status, BranchStatus::Passed, "{:?}", b.error);
    recorded(fx, id).await.0
}

/// Runs `plan`'s branch in a worker linked through `link`, handing it `ticket` once it waits:
/// the worker's exit code.
async fn run_worker(link: &str, ticket: &str, toys: &Path) -> Option<i32> {
    run_worker_with(link, ticket, toys, &[]).await
}

/// `run_worker` with more environment `env`.
async fn run_worker_with(
    link: &str,
    ticket: &str,
    toys: &Path,
    env: &[(&str, &str)],
) -> Option<i32> {
    let scratch = tempfile::tempdir().unwrap();
    let file = scratch.path().join("ticket");
    let mut worker = Worker::start(link, &file, toys, scratch.path(), env);
    tokio::time::sleep(Duration::from_millis(300)).await;
    hand_over(&file, ticket);
    let code = worker.exit_code().await;
    assert!(!file.exists(), "the worker deletes its ticket file");
    code
}

/// Writes `ticket` to `file` as the worker asks: to a temporary file beside it, newline-ended,
/// then renamed.
fn hand_over(file: &Path, ticket: &str) {
    let partial = file.with_extension("partial");
    std::fs::write(&partial, format!("{ticket}\n")).unwrap();
    std::fs::rename(&partial, file).unwrap();
}

/// Asserts the worker's branch ended `passed` with the kinds of a local run, each event once.
async fn assert_like_local(fx: &AppFx, plan: &BranchPlan, team: &Team) {
    let b = live::ended(&fx.state, &plan.branch_id).await;
    assert_eq!(b.status, BranchStatus::Passed, "{:?}", b.error);
    let (kinds, seqs) = recorded(fx, &plan.branch_id).await;
    assert_eq!(
        kinds.first(),
        Some(&EventKind::RunStart),
        "RunStart is seq 1"
    );
    assert_eq!(seqs, (1..=seqs.len() as i64).collect::<Vec<_>>());
    assert_eq!(kinds, local_kinds(fx, team).await);
}

#[test]
fn a_worker_runs_the_toy_world_end_to_end() {
    run(async {
        let (fx, toys) = control("e2e").await;
        let team = scripted_team(nothing()).await;
        let (plan, ticket) = branch(&fx, team.clone()).await;
        let code = run_worker(&ws_url(&fx.base), &ticket, &toys).await;
        assert_eq!(code, Some(0));
        assert_like_local(&fx, &plan, &team).await;
    });
}

/// A TCP relay to the control box: its address, how many connections it took, and a sender that
/// cuts every connection it relays now. Once closed it cuts them and takes no more.
struct Relay {
    addr: String,
    /// Where it relays to (`host:port`).
    to: Arc<std::sync::Mutex<String>>,
    cut: broadcast::Sender<()>,
    accepted: Arc<AtomicUsize>,
    closed: Arc<AtomicBool>,
}

impl Relay {
    async fn to(fx: &AppFx) -> Relay {
        let to = fx.base.trim_start_matches("http://").to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay = Relay {
            addr: listener.local_addr().unwrap().to_string(),
            to: Arc::new(std::sync::Mutex::new(to)),
            cut: broadcast::channel::<()>(4).0,
            accepted: Arc::default(),
            closed: Arc::default(),
        };
        let (cut, accepted, closed) = (
            relay.cut.clone(),
            relay.accepted.clone(),
            relay.closed.clone(),
        );
        let to = relay.to.clone();
        tokio::spawn(async move {
            loop {
                let (inbound, _) = listener.accept().await.unwrap();
                if closed.load(Ordering::SeqCst) {
                    continue;
                }
                accepted.fetch_add(1, Ordering::SeqCst);
                let target = to.lock().unwrap().clone();
                tokio::spawn(relayed(inbound, target, cut.subscribe()));
            }
        });
        relay
    }

    fn link(&self) -> String {
        format!("ws://{}/worker", self.addr)
    }

    /// Cuts every connection; new ones go to `to` (`host:port`).
    fn redirect(&self, to: &str) {
        *self.to.lock().unwrap() = to.to_string();
        let _ = self.cut.send(());
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let _ = self.cut.send(());
    }
}

async fn relayed(mut inbound: TcpStream, to: String, mut cut: broadcast::Receiver<()>) {
    let mut out = TcpStream::connect(&to).await.unwrap();
    tokio::select! {
        _ = tokio::io::copy_bidirectional(&mut inbound, &mut out) => {}
        _ = cut.recv() => {}
    }
}

#[test]
fn a_worker_rejoins_after_its_link_drops_and_loses_and_doubles_nothing() {
    run(async {
        let (fx, toys) = control("cut").await;
        let relay = Relay::to(&fx).await;
        let cut = relay.cut.clone();
        let team = scripted_team(Arc::new(move || {
            let _ = cut.send(());
            Box::pin(async {})
        }))
        .await;
        let (plan, ticket) = branch(&fx, team.clone()).await;
        let code = run_worker(&relay.link(), &ticket, &toys).await;
        assert_eq!(code, Some(0));
        assert!(
            relay.accepted.load(Ordering::SeqCst) >= 2,
            "the link was cut and joined again"
        );
        assert_like_local(&fx, &plan, &team).await;
    });
}

#[test]
fn an_event_too_big_for_a_frame_is_recorded_cut_down() {
    run(async {
        let (fx, _toys) = control("big").await;
        let (plan, ticket) = branch(&fx, scripted_team(nothing()).await).await;
        let o = open(&ws_url(&fx.base), &ticket, Patience::default()).await;
        let o = o.unwrap();
        let big = json!({"text": "x".repeat(MAX_LINK_FRAME)});
        let ev = o.records.append(None, EventKind::Message, big, None).await;
        let ev = ev.unwrap();
        assert_eq!(ev.seq, 1);
        o.records
            .set_status(BranchStatus::Passed, None)
            .await
            .unwrap();
        o.link.await.unwrap().unwrap();
        let evs = records(&fx.state.store)
            .events(&plan.branch_id)
            .await
            .unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].payload["truncated"], json!(true));
        assert_eq!(evs[0].payload["text"].as_str().unwrap().len(), KEPT_STRING);
        assert_eq!(
            evs[0].payload, ev.payload,
            "the worker keeps what the control box has"
        );
        let b = live::ended(&fx.state, &plan.branch_id).await;
        assert_eq!(b.status, BranchStatus::Passed);
    });
}

#[test]
fn a_worker_that_cannot_rejoin_gives_up() {
    run(async {
        let (fx, _toys) = control("giveup").await;
        let relay = Relay::to(&fx).await;
        let (_, ticket) = branch(&fx, scripted_team(nothing()).await).await;
        let patience = Patience {
            retry: Duration::from_millis(100),
            give_up: Duration::from_millis(800),
        };
        let o = open(&relay.link(), &ticket, patience).await.unwrap();
        relay.close();
        let appended = o.records.append(None, EventKind::Message, json!({}), None);
        assert!(
            appended.await.is_err(),
            "an event the link never took fails"
        );
        let err = o.link.await.unwrap().unwrap_err();
        assert!(format!("{err:#}").contains("gave up"), "{err:#}");
    });
}

#[test]
fn stop_pressed_on_the_control_box_stops_the_workers_branch() {
    run(async {
        let (fx, toys) = control("stop").await;
        let (base, id) = (fx.base.clone(), Arc::new(OnceLock::<String>::new()));
        let branch_id = id.clone();
        let team = scripted_team(Arc::new(move || {
            let (base, id) = (base.clone(), branch_id.clone());
            Box::pin(async move {
                let url = format!("{base}/api/branches/{}/stop", id.get().unwrap());
                let (status, v) =
                    app::send(reqwest::Client::new().post(url).json(&json!({}))).await;
                assert_eq!(status, 200, "{v}");
            })
        }))
        .await;
        let (plan, ticket) = branch(&fx, team).await;
        id.set(plan.branch_id.clone()).unwrap();
        let code = run_worker(&ws_url(&fx.base), &ticket, &toys).await;
        assert_eq!(code, Some(0));
        let b = live::ended(&fx.state, &plan.branch_id).await;
        assert_eq!(b.status, BranchStatus::Stopped, "{:?}", b.error);
    });
}

/// A `/worker` that accepts every WebSocket upgrade and closes it, as the control box does when it
/// cannot resolve a token or append (its database down): its `host:port` and how many connections
/// it took. With `ping` it first pings, then reads the hello and the event sent again, and closes
/// on that event (a control box that can ping but not append).
async fn closing_worker_endpoint(ping: bool) -> (String, Arc<AtomicUsize>) {
    use axum::extract::ws::{Message, WebSocketUpgrade};
    let taken = Arc::new(AtomicUsize::new(0));
    let count = taken.clone();
    let app = Router::new().route(
        "/worker",
        axum::routing::get(move |ws: WebSocketUpgrade| {
            count.fetch_add(1, Ordering::SeqCst);
            async move {
                ws.on_upgrade(move |mut s| async move {
                    if ping {
                        let _ = s.send(Message::Ping(Default::default())).await;
                        let _ = s.recv().await; // hello
                        let _ = s.recv().await; // the event, which it cannot append
                    }
                    let _ = s.send(Message::Close(None)).await;
                })
            }
        }),
    );
    let (listener, base) = app::bind().await;
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base.trim_start_matches("http://").to_string(), taken)
}

/// Opens a link, points it at a closing endpoint (`ping` as there) with an event waiting for its
/// ack, and asserts the link gives up within the 1 s test give-up, one handshake per retry.
async fn gives_up_against_closing(tag: &str, ping: bool) {
    let (fx, _toys) = control(tag).await;
    let relay = Relay::to(&fx).await;
    let (closing, taken) = closing_worker_endpoint(ping).await;
    let (_, ticket) = branch(&fx, scripted_team(nothing()).await).await;
    let patience = Patience {
        retry: Duration::from_millis(100),
        give_up: Duration::from_millis(1000),
    };
    let o = open(&relay.link(), &ticket, patience).await.unwrap();
    relay.redirect(&closing);
    let since = std::time::Instant::now();
    let appended = o.records.append(None, EventKind::Message, json!({}), None);
    let appended = tokio::time::timeout(Duration::from_secs(5), appended).await;
    let appended = appended.expect("the link gives up within 5 s");
    assert!(appended.is_err(), "an event the link never took fails");
    let err = o.link.await.unwrap().unwrap_err();
    assert!(format!("{err:#}").contains("gave up"), "{err:#}");
    let (took, joins) = (since.elapsed(), taken.load(Ordering::SeqCst));
    assert!(took < Duration::from_secs(3), "gave up after {took:?}");
    let one_per_retry = (2..=12).contains(&joins);
    assert!(one_per_retry, "{joins} handshakes in {took:?}");
}

#[test]
fn a_link_that_is_accepted_then_closed_still_gives_up_and_does_not_spin() {
    run(gives_up_against_closing("closing", false));
}

#[test]
fn a_link_that_pings_but_never_acks_still_gives_up() {
    run(gives_up_against_closing("pinging", true));
}

#[test]
fn a_worker_whose_link_is_gone_for_good_exits_1() {
    run(async {
        let (fx, toys) = control("exit1").await;
        let relay = Arc::new(Relay::to(&fx).await);
        let closer = relay.clone();
        let team = scripted_team(Arc::new(move || {
            closer.close();
            Box::pin(async {})
        }))
        .await;
        let (_, ticket) = branch(&fx, team).await;
        let env = [
            ("SHAKEDOWN_WORKER_RETRY_MS", "100"),
            ("SHAKEDOWN_WORKER_GIVE_UP_MS", "1500"),
        ];
        let code = run_worker_with(&relay.link(), &ticket, &toys, &env).await;
        assert_eq!(code, Some(1));
    });
}
