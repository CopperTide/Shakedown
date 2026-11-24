//! `shaked worker` end to end. A test control box (no `Hosting`) makes a branch with
//! `record_start` and issues a ticket for it; a `shaked worker` child process takes the ticket
//! from a file, connects to the control box's `/worker`, stands the toy world up and drives one
//! scripted agent (an `http` agent, `support/scripted.rs`, no model), recording every event
//! through the link. The branch ends on the control box as a local run of the same team ends.
mod support;
use support::fake_apis::{
    fake_apis, store_org_keys, FakeApis, COMPLETED, ORG_CLAUDE_KEY, ORG_ROUTER_KEY,
};
use support::link::{self, ws_url};
use support::scripted::{mcp, nothing, scripted_team};
use support::{app, live, packages, poll_until, records, run, AppFx, STEP};

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
use serde_json::{json, Value};
use shaked::{
    exec::BranchRecords,
    model::*,
    runner::BranchPlan,
    worker::{
        link::MAX_LINK_FRAME,
        link_records::KEPT_STRING,
        run::{open, Patience},
    },
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::broadcast,
};

/// A control box on the scored toy world (task `t1`, solved by one poke), and the folder of
/// the toy packages.
async fn control(tag: &str) -> (AppFx, PathBuf) {
    let root = packages::toy_world(&format!("wrun-{tag}"), &[], &["--scored"]);
    let dir = root.0.clone();
    let fx = AppFx::idle("worker_run", tag).root(root).bench("toybench");
    (fx.start().await, dir)
}

/// A branch of `t1` made by `record_start` for `team`, expected on the link, and a ticket for it.
async fn branch(fx: &AppFx, team: Team) -> (BranchPlan, String) {
    link::ticketed(fx, "t1", team, Rules::new(Runner::Chain)).await
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

    /// Its exit code, once it exits (within three minutes).
    async fn exit_code(&mut self) -> Option<i32> {
        let limit = Duration::from_secs(180);
        let exited = poll_until(limit, STEP, async || self.0.try_wait().unwrap()).await;
        exited.expect("the worker did not exit").code()
    }
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

/// What the model-calling scripted agents got: each one's token and its answer's text, in turn.
type Answers = Arc<std::sync::Mutex<Vec<(String, String)>>>;

/// The scripted agents of a worker's model calls, through their gateway: `a` calls Claude's
/// Messages API with its token as `x-api-key` (as Claude Code sends it), `c` OpenAI's Responses
/// API streamed (on OpenRouter, whose calls report their cost) with its token as the bearer (as
/// Codex does), then pokes the box once.
async fn calls_models(State(answers): State<Answers>, Json(run): Json<Value>) -> Json<Value> {
    if run["type"] == "hello" {
        return Json(json!({}));
    }
    let (gateway, token) = (
        run["gateway_url"].as_str().unwrap(),
        run["token"].as_str().unwrap(),
    );
    let text = match run["agent"] == "a" {
        true => ask_claude(gateway, token).await,
        false => ask_openai(gateway, token).await,
    };
    answers.lock().unwrap().push((token.to_string(), text));
    if run["agent"] == "c" {
        let call = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                          "params": {"name": "poke", "arguments": {}}});
        mcp(run["mcp_url"].as_str().unwrap(), token, call).await;
    }
    Json(json!({"ok": true}))
}

/// Claude's answer to one Messages call through `gateway`, the token as `x-api-key`.
async fn ask_claude(gateway: &str, token: &str) -> String {
    let ask = json!({"model": "claude-sonnet-5-5", "max_tokens": 8,
                     "messages": [{"role": "user", "content": "hi"}]});
    let req = reqwest::Client::new().post(format!("{gateway}/messages"));
    let req = req
        .header("x-api-key", token)
        .header("anthropic-version", "2023-06-01");
    let r = req.json(&ask).send().await.unwrap();
    r.text().await.unwrap()
}

/// The whole stream of one streamed Responses call through `gateway`, the token the bearer.
async fn ask_openai(gateway: &str, token: &str) -> String {
    let ask = json!({"model": "gpt-x", "stream": true, "input": "hi"});
    let req = reqwest::Client::new().post(format!("{gateway}/responses"));
    let r = req.bearer_auth(token).json(&ask).send().await.unwrap();
    r.text().await.unwrap()
}

/// The model-calling scripted agent served on a free port: the team of it alone.
async fn model_calling_team(answers: Answers) -> Team {
    let app = Router::new()
        .route("/run", post(calls_models))
        .with_state(answers);
    let (listener, base) = app::bind().await;
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let url = format!("{base}/run");
    let a = json!({"id": "a", "name": "A", "kind": "http", "url": url});
    let c = json!({"id": "c", "name": "C", "kind": "http", "url": url, "provider": "openrouter"});
    serde_json::from_value(json!({"id": "t", "name": "T", "agents": [a, c]})).unwrap()
}

/// What reached the spy: each request's path and headers.
type Left = Arc<std::sync::Mutex<Vec<(String, axum::http::HeaderMap)>>>;

/// An HTTP relay to the control box at `to` that keeps the path and headers of each request it
/// relays: everything that leaves the worker for the control box's gateway.
async fn spy(
    State((to, left)): State<(String, Left)>,
    req: axum::extract::Request,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let (parts, body) = req.into_parts();
    let path = parts.uri.path().to_string();
    left.lock()
        .unwrap()
        .push((path.clone(), parts.headers.clone()));
    let body = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    let mut headers = parts.headers;
    headers.remove(axum::http::header::HOST);
    let req = reqwest::Client::new()
        .post(format!("{to}{path}"))
        .headers(headers);
    let r = req.body(body).send().await.unwrap();
    let (status, kind) = (r.status(), r.headers()["content-type"].clone());
    let body = r.bytes().await.unwrap();
    (status, [(axum::http::header::CONTENT_TYPE, kind)], body).into_response()
}

/// A control box on the toy world whose model providers are `up`, with the org's keys stored,
/// handing its workers the gateway at `public` (a spy in front of it).
async fn keyed_control(tag: &str, up: &FakeApis, public: String) -> (AppFx, PathBuf) {
    let root = packages::toy_world(&format!("wrun-{tag}"), &[], &["--scored"]);
    let dir = root.0.clone();
    let fx = AppFx::idle("worker_run", tag).root(root).bench("toybench");
    let fx = fx
        .providers(up.providers())
        .config(move |c| c.public_url = public);
    let fx = fx.start().await;
    store_org_keys(&fx).await;
    (fx, dir)
}

/// No request that left the worker carries a model key or the agent's token: each is a
/// gateway call of its branch's agent with the worker token alone.
fn only_the_worker_token_left(
    left: &[(String, axum::http::HeaderMap)],
    branch: &str,
    agent_tokens: &[&str],
) {
    let paths: Vec<&str> = left.iter().map(|(p, _)| p.as_str()).collect();
    let want = [
        format!("/gateway/{branch}/a/v1/messages"),
        format!("/gateway/{branch}/c/v1/responses"),
    ];
    assert_eq!(paths, want, "one request per model call");
    for (path, h) in left {
        let auth = h["authorization"].to_str().unwrap();
        assert!(auth.starts_with("Bearer sdw_"), "{auth}");
        assert!(h.get("x-api-key").is_none(), "{path}");
        for (name, v) in h {
            let v = v.to_str().unwrap_or_default();
            assert!(
                !v.contains("sk-") && !agent_tokens.iter().any(|t| v.contains(t)),
                "{name}: {v}"
            );
        }
    }
}

/// The providers saw the org's stored keys, and only them.
fn the_providers_saw_the_orgs_keys(up: &FakeApis) {
    let seen = up.seen();
    let paths: Vec<&str> = seen.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, ["/v1/messages", "/v1/responses"]);
    assert_eq!(seen[0].headers["x-api-key"], ORG_CLAUDE_KEY);
    assert!(seen[0].headers.get("authorization").is_none());
    let auth = seen[1].headers["authorization"].to_str().unwrap();
    assert_eq!(auth, format!("Bearer {ORG_ROUTER_KEY}"));
    assert!(seen[1].headers.get("x-api-key").is_none());
}

/// Model keys a sandbox's environment might have: the worker sends none of them.
const SANDBOX_KEYS: [(&str, &str); 2] = [
    ("OPENAI_API_KEY", "sk-sandbox-env-3333"),
    ("ANTHROPIC_API_KEY", "sk-ant-sandbox-env-4444"),
];

/// The spy served on `listener`, relaying to `to`: what it relays.
fn spying(listener: tokio::net::TcpListener, to: &str) -> Left {
    let left: Left = Arc::default();
    let relay = Router::new().fallback(spy);
    let relay = relay.with_state((to.to_string(), left.clone()));
    tokio::spawn(async move { axum::serve(listener, relay).await.unwrap() });
    left
}

#[test]
fn a_workers_model_calls_carry_no_model_key_and_are_recorded_once() {
    run(async {
        let up = fake_apis().await;
        up.gate.add_permits(10);
        let (listener, public) = app::bind().await;
        let (fx, toys) = keyed_control("keys", &up, public).await;
        let left = spying(listener, &fx.base);
        let answers: Answers = Arc::default();
        let (plan, ticket) = branch(&fx, model_calling_team(answers.clone()).await).await;
        let code = run_worker_with(&ws_url(&fx.base), &ticket, &toys, &SANDBOX_KEYS).await;
        assert_eq!(code, Some(0));
        let b = live::ended(&fx.state, &plan.branch_id).await;
        assert_eq!(b.status, BranchStatus::Passed, "{:?}", b.error);
        let got = answers.lock().unwrap().clone();
        assert!(got[0].1.contains("hello from claude"), "{}", got[0].1);
        assert!(got[1].1.ends_with(COMPLETED), "the stream came whole");
        let tokens = [got[0].0.as_str(), got[1].0.as_str()];
        only_the_worker_token_left(&left.lock().unwrap(), &plan.branch_id, &tokens);
        the_providers_saw_the_orgs_keys(&up);
        let (kinds, _) = recorded(&fx, &plan.branch_id).await;
        let calls = kinds.iter().filter(|k| **k == EventKind::ModelCall).count();
        assert_eq!(calls, 2, "each call recorded once, on the control box");
    });
}
