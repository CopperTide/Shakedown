//! shaked's recording proxy: agents' own HTTP clients reach the hosts a world's parts serve, and
//! each request is recorded as an `http` step of the part serving the host. The world is a toy
//! part `box` whose one route sends `toy.test` to a small axum upstream the test runs; the run is
//! one an external system drives (no model answers anything here).
mod support;
use support::records;

use std::{collections::HashMap, time::Duration};

use axum::{
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use shaked::model::*;
use support::{packages, run, AppFx};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// The request's `host` and `x-forwarded-*` headers, as JSON.
async fn echo_host(h: HeaderMap) -> axum::Json<Value> {
    let names = [
        "host",
        "x-forwarded-host",
        "x-forwarded-proto",
        "x-forwarded-port",
        "x-forwarded-for",
        "forwarded",
        "x-shakedown-effect",
        "x-shakedown-agent",
        "x-shakedown-readonly",
    ];
    let got: HashMap<&str, &str> = (names.iter())
        .filter_map(|n| Some((*n, h.get(*n)?.to_str().ok()?)))
        .collect();
    axum::Json(json!(got))
}

/// The request's `authorization` header.
async fn echo_auth(h: HeaderMap) -> String {
    let auth = h.get("authorization").map(|v| v.to_str().unwrap());
    auth.unwrap_or("").to_string()
}

/// The upstream the toy part routes `toy.test` to.
async fn upstream() -> String {
    let app = upstream_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

/// The upstream's routes.
fn upstream_app() -> Router {
    let graphql = || async { ([("x-shakedown-effect", "read")], "{\"data\":{}}") };
    Router::new()
        .route("/hello", get(|| async { "hi" }))
        .route("/echo-host", get(echo_host))
        .route("/echo-auth", get(echo_auth))
        .route(
            "/items",
            post(|b: String| async { (StatusCode::CREATED, b) }),
        )
        .route("/graphql", post(graphql))
        .route("/hang", get(std::future::pending::<String>))
        .route(
            "/read",
            post(|h| async { ([("x-shakedown-effect", "read")], echo_host(h).await) }),
        )
        .route(
            "/write",
            post(|| async {
                (
                    StatusCode::CREATED,
                    [("x-shakedown-effect", "write")],
                    "done",
                )
            }),
        )
}

/// An upstream that answers `GET /name` with `name` and takes `POST /items`.
async fn named_upstream(name: &'static str) -> String {
    let app = Router::new()
        .route("/name", get(move || async move { name }))
        .route("/items", post(|| async { StatusCode::CREATED }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

/// A port nothing listens on.
fn closed_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// An app whose world is a toy part `box` serving `toy.test` at `upstream`, and a live external
/// run of task `t1` with the agents `planner` and (read-only) `reader`.
struct ProxyFx {
    fx: AppFx,
    branch: String,
}

impl ProxyFx {
    async fn new(tag: &str, upstream: &str, part_args: &[&str]) -> ProxyFx {
        let route = format!("toy.test={upstream}");
        let mut args = vec!["--http-route", route.as_str()];
        args.extend_from_slice(part_args);
        ProxyFx::with_part(tag, &args).await
    }

    /// The app over a toy part `box` started with `part_args` (no route unless they give one).
    async fn with_part(tag: &str, part_args: &[&str]) -> ProxyFx {
        ProxyFx::configured(tag, part_args, |_| {}).await
    }

    /// `with_part` with the app's config changed by `tweak`.
    async fn configured(
        tag: &str,
        part_args: &[&str],
        tweak: impl FnOnce(&mut shaked::app::Config) + Send + 'static,
    ) -> ProxyFx {
        let root = packages::toy_world(tag, part_args, &[]);
        let fx = AppFx::idle("proxy", tag)
            .root(root)
            .bench("toybench")
            .config(tweak)
            .start()
            .await;
        let team = json!({"id": "own", "name": "Own design", "agents": []});
        let body = json!({"task_id": "t1", "team": team, "runner": "external"});
        let (status, v) = fx.post("/api/runs", body).await;
        assert_eq!(status, 200, "{v}");
        let branch = v["branch_id"].as_str().unwrap().to_string();
        let pfx = ProxyFx { fx, branch };
        pfx.join(json!({"id": "planner", "name": "Planner"})).await;
        pfx.join(json!({"id": "reader", "name": "Reader", "read_only": true}))
            .await;
        pfx
    }

    /// Registers `agent`: the register answer.
    async fn join(&self, agent: Value) -> Value {
        let path = format!("/api/branches/{}/agents", self.branch);
        let (status, a) = self.fx.post(&path, agent).await;
        assert_eq!(status, 200, "{a}");
        a
    }

    /// A ticket of `agent` on the branch.
    fn ticket(&self, agent: Option<&str>) -> String {
        self.fx
            .state
            .proxy
            .tickets
            .issue(&self.branch, agent, later())
    }

    /// A client that goes through the proxy as `agent`.
    fn client(&self, agent: &str) -> reqwest::Client {
        let url = self.fx.state.proxy.url(&self.ticket(Some(agent))).unwrap();
        let proxy = reqwest::Proxy::all(url).unwrap();
        reqwest::Client::builder().proxy(proxy).build().unwrap()
    }

    async fn events(&self) -> Vec<Event> {
        records(&self.fx.state.store)
            .events(&self.branch)
            .await
            .unwrap()
    }

    async fn last_tool_call(&self) -> Event {
        let evs = self.events().await;
        let mut calls = evs.into_iter().filter(|e| e.kind == EventKind::ToolCall);
        calls.next_back().expect("a tool call")
    }

    async fn on(&self, what: &str) -> (u16, Value) {
        let path = format!("/api/branches/{}/{what}", self.branch);
        self.fx.post(&path, json!({})).await
    }
}

async fn get_text(c: &reqwest::Client, url: &str) -> (u16, String) {
    let r = c.get(url).send().await.unwrap();
    (r.status().as_u16(), r.text().await.unwrap())
}

fn sha256_hex(b: &[u8]) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, b);
    d.as_ref().iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn a_client_cannot_set_the_forwarding_headers() {
    run(async {
        let fx = ProxyFx::new("spoof", &upstream().await, &[]).await;
        let r = (fx.client("planner").get("http://toy.test/echo-host"))
            .header("x-forwarded-host", "evil.test")
            .header("X-Forwarded-Proto", "https")
            .header("x-forwarded-for", "10.0.0.1")
            .header("forwarded", "host=evil.test")
            .header("x-shakedown-effect", "read")
            .send()
            .await;
        let v: Value = r.unwrap().json().await.unwrap();
        let want = json!({"host": "toy.test", "x-forwarded-host": "toy.test",
                          "x-forwarded-proto": "http", "x-forwarded-port": "80",
                          "x-shakedown-agent": "planner"});
        assert_eq!(v, want);
    })
}

#[test]
fn a_get_through_the_proxy_is_a_read_step_of_the_part_serving_the_host() {
    run(async {
        let fx = ProxyFx::new("get", &upstream().await, &[]).await;
        let got = get_text(&fx.client("planner"), "http://toy.test/hello").await;
        assert_eq!(got, (200, "hi".to_string()));
        let e = fx.last_tool_call().await;
        let who = (e.agent.as_deref(), &e.payload["name"], &e.payload["part"]);
        assert_eq!(who, (Some("planner"), &json!("http"), &json!("box")));
        assert_eq!(e.payload["args"]["method"], "GET");
        assert_eq!(e.payload["args"]["url"], "http://toy.test/hello");
        let result = &e.payload["result"];
        assert_eq!(
            (&result["status"], &result["body"]["text"]),
            (&json!(200), &json!("hi"))
        );
        let effect = (&e.payload["reads"], &e.payload["writes"]);
        assert_eq!(effect, (&json!(["http/toy.test"]), &json!([])));
        assert!(e.payload.get("saves").is_none());
    })
}

#[test]
fn a_post_is_a_write_and_saves_its_part() {
    run(async {
        let fx = ProxyFx::new("post", &upstream().await, &[]).await;
        let c = fx.client("planner");
        let r = c
            .post("http://toy.test/items")
            .body("{\"n\":1}")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 201);
        assert_eq!(r.text().await.unwrap(), "{\"n\":1}");
        let e = fx.last_tool_call().await;
        assert_eq!(e.payload["writes"], json!(["http/toy.test"]));
        assert_eq!(e.payload["args"]["body"]["text"], "{\"n\":1}");
        assert!(e.payload["saves"]["box"].is_string(), "{}", e.payload);
    })
}

#[test]
fn the_upstream_sees_the_worlds_host_name() {
    run(async {
        let fx = ProxyFx::new("host", &upstream().await, &[]).await;
        let r = fx
            .client("planner")
            .get("http://toy.test/echo-host")
            .send()
            .await;
        let v: Value = r.unwrap().json().await.unwrap();
        let want = json!({"host": "toy.test", "x-forwarded-host": "toy.test",
                          "x-forwarded-proto": "http", "x-forwarded-port": "80",
                          "x-shakedown-agent": "planner"});
        assert_eq!(v, want);
    })
}

/// A raw request to the proxy at `addr`: the head, then (after `100 Continue`) the body in
/// chunks of 16 KiB. The status line of the final answer.
async fn post_chunked_with_expect(addr: std::net::SocketAddr, ticket: &str, body: &[u8]) -> String {
    use base64::Engine;
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("shakedown:{ticket}"));
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let head = format!(
        "POST http://toy.test/items HTTP/1.1\r\nHost: toy.test\r\nProxy-Authorization: Basic {auth}\r\n\
         Expect: 100-continue\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
    );
    s.write_all(head.as_bytes()).await.unwrap();
    let (r, mut w) = s.into_split();
    let mut r = BufReader::new(r);
    let mut line = String::new();
    r.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("HTTP/1.1 100"), "{line}");
    for chunk in body.chunks(16 << 10) {
        w.write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
            .await
            .unwrap();
        w.write_all(chunk).await.unwrap();
        w.write_all(b"\r\n").await.unwrap();
    }
    w.write_all(b"0\r\n\r\n").await.unwrap();
    let mut rest = String::new();
    let mut buf = vec![];
    r.read_to_end(&mut buf).await.unwrap();
    rest.push_str(&String::from_utf8_lossy(&buf));
    let after_continue = rest.trim_start_matches("\r\n");
    after_continue.lines().next().unwrap_or("").to_string()
}

#[test]
fn a_large_post_with_expect_continue_goes_through() {
    run(async {
        let fx = ProxyFx::new("large", &upstream().await, &[]).await;
        let body: Vec<u8> = (0..204_800u32).map(|i| b'a' + (i % 26) as u8).collect();
        let addr = fx.fx.state.proxy.addr().unwrap();
        let status = post_chunked_with_expect(addr, &fx.ticket(Some("planner")), &body).await;
        assert!(status.starts_with("HTTP/1.1 201"), "{status}");
        let e = fx.last_tool_call().await;
        assert_eq!(e.payload["args"]["body"]["bytes"], 204_800);
        assert_eq!(e.payload["args"]["body"]["sha256"], sha256_hex(&body));
        assert!(e.payload["args"]["body"].get("text").is_none());
    })
}

#[test]
fn an_unknown_host_is_refused_and_recorded() {
    run(async {
        let fx = ProxyFx::new("unknown", &upstream().await, &[]).await;
        let got = get_text(&fx.client("planner"), "http://other.test/").await;
        let refused = "the host other.test is not part of this world";
        assert_eq!(got, (403, refused.to_string()));
        let e = fx.last_tool_call().await;
        assert_eq!(e.payload["error"], refused);
        assert!(e.payload.get("part").is_none(), "{}", e.payload);
    })
}

#[test]
fn a_request_without_the_ticket_is_turned_away_unrecorded() {
    run(async {
        let fx = ProxyFx::new("noticket", &upstream().await, &[]).await;
        let before = fx.events().await.len();
        let addr = fx.fx.state.proxy.addr().unwrap();
        let bare = reqwest::Proxy::all(format!("http://{addr}")).unwrap();
        let wrong = reqwest::Proxy::all(format!("http://shakedown:nope@{addr}")).unwrap();
        for proxy in [bare, wrong] {
            let c = reqwest::Client::builder().proxy(proxy).build().unwrap();
            let r = c.get("http://toy.test/hello").send().await.unwrap();
            assert_eq!(r.status().as_u16(), 407);
            assert!(r.headers().contains_key("proxy-authenticate"));
            let text = r.text().await.unwrap();
            assert_eq!(
                text,
                "this proxy wants the ticket in the proxy URL Shakedown gave"
            );
        }
        assert_eq!(fx.events().await.len(), before);
    })
}

#[test]
fn an_origin_form_request_is_told_to_send_absolute_urls() {
    run(async {
        let fx = ProxyFx::new("origin", &upstream().await, &[]).await;
        use base64::Engine;
        let ticket = format!("shakedown:{}", fx.ticket(Some("planner")));
        let auth = base64::engine::general_purpose::STANDARD.encode(ticket);
        let addr = fx.fx.state.proxy.addr().unwrap();
        let r = fx.fx.http.get(format!("http://{addr}/hello"));
        let r = r
            .header("proxy-authorization", format!("Basic {auth}"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 400);
        let text = r.text().await.unwrap();
        assert_eq!(text, "this is Shakedown's proxy: send absolute URLs");
    })
}

#[test]
fn a_read_only_agent_may_read_but_not_write() {
    run(async {
        let fx = ProxyFx::new("readonly", &upstream().await, &[]).await;
        let c = fx.client("reader");
        assert_eq!(get_text(&c, "http://toy.test/hello").await.0, 200);
        let r = c
            .post("http://toy.test/items")
            .body("x")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 403);
        let refused = "Write requests are not available to this agent: its team made it read-only";
        assert_eq!(r.text().await.unwrap(), refused);
        let e = fx.last_tool_call().await;
        assert_eq!(
            (e.agent.as_deref(), &e.payload["error"]),
            (Some("reader"), &json!(refused))
        );
        assert!(e.payload.get("saves").is_none());
    })
}

#[test]
fn the_upstream_is_told_which_agent_sent_each_request() {
    run(async {
        let fx = ProxyFx::new("agent", &upstream().await, &[]).await;
        for agent in ["planner", "reader"] {
            let r = (fx.client(agent).get("http://toy.test/echo-host"))
                .header("x-shakedown-agent", "boss")
                .header("x-shakedown-readonly", "0")
                .send()
                .await;
            let v: Value = r.unwrap().json().await.unwrap();
            // A client's own copies are dropped.
            assert_eq!(v["x-shakedown-agent"], json!(agent), "{v}");
            assert!(v.get("x-shakedown-readonly").is_none(), "{v}");
        }
    })
}

#[test]
fn a_read_only_post_goes_marked_to_a_route_that_honours_read_only() {
    run(async {
        let honours = ["--route-honours-readonly"];
        let fx = ProxyFx::new("honours", &upstream().await, &honours).await;
        let (status, v) = post_json(&fx.client("reader"), "http://toy.test/read").await;
        assert_eq!(status, 200);
        assert_eq!(
            (&v["x-shakedown-agent"], &v["x-shakedown-readonly"]),
            (&json!("reader"), &json!("1"))
        );
        let e = fx.last_tool_call().await;
        assert!(e.payload["error"].is_null(), "{}", e.payload);
        assert_eq!(e.payload["reads"], json!(["http/toy.test"]));
        let (_, v) = post_json(&fx.client("planner"), "http://toy.test/read").await;
        assert!(
            v.get("x-shakedown-readonly").is_none(),
            "only a read-only agent's: {v}"
        );
    })
}

/// An empty POST to `url`: the status and the JSON answer.
async fn post_json(c: &reqwest::Client, url: &str) -> (u16, Value) {
    let r = c.post(url).send().await.unwrap();
    (r.status().as_u16(), r.json().await.unwrap())
}

#[test]
fn a_read_only_request_the_part_says_wrote_is_a_violation_that_fails_the_branch() {
    run(async {
        let honours = ["--route-honours-readonly"];
        let fx = ProxyFx::new("violation", &upstream().await, &honours).await;
        let r = fx
            .client("reader")
            .post("http://toy.test/write")
            .send()
            .await;
        assert_eq!(r.unwrap().status().as_u16(), 201);
        let e = fx.last_tool_call().await;
        let why =
            "a read-only agent's request wrote: the box part answered X-Shakedown-Effect: write \
                   to POST http://toy.test/write";
        assert_eq!(
            (e.agent.as_deref(), &e.payload["error"]),
            (Some("reader"), &json!(why))
        );
        assert_eq!(e.payload["writes"], json!(["http/toy.test"]));
        assert_eq!(fx.on("finish").await.0, 200);
        let b = records(&fx.fx.state.store).branch(&fx.branch).await;
        let b = b.unwrap();
        assert_eq!(
            (b.status, b.error.as_deref()),
            (BranchStatus::Failed, Some(why))
        );
    })
}

#[test]
fn secrets_in_headers_are_not_recorded() {
    run(async {
        let fx = ProxyFx::new("secrets", &upstream().await, &[]).await;
        let c = fx.client("planner");
        let r = c
            .get("http://toy.test/echo-auth")
            .header("authorization", "token abc");
        assert_eq!(r.send().await.unwrap().text().await.unwrap(), "token abc");
        let e = fx.last_tool_call().await;
        assert_eq!(e.payload["args"]["headers"]["authorization"], "(redacted)");
        assert!(e.payload["args"]["headers"]
            .get("proxy-authorization")
            .is_none());
    })
}

#[test]
fn the_part_says_when_a_post_only_read() {
    run(async {
        let fx = ProxyFx::new("graphql", &upstream().await, &[]).await;
        let c = fx.client("planner");
        let r = c
            .post("http://toy.test/graphql")
            .body("{\"query\":\"{a}\"}")
            .send()
            .await;
        let r = r.unwrap();
        assert_eq!(r.status().as_u16(), 200);
        assert!(!r.headers().contains_key("x-shakedown-effect"));
        let e = fx.last_tool_call().await;
        let effect = (&e.payload["reads"], &e.payload["writes"]);
        assert_eq!(effect, (&json!(["http/toy.test"]), &json!([])));
        assert!(e.payload.get("saves").is_none());
    })
}

#[test]
fn a_route_pattern_makes_a_post_a_read() {
    run(async {
        let reads = ["--route-reads", "POST */items"];
        let fx = ProxyFx::new("pattern", &upstream().await, &reads).await;
        let c = fx.client("planner");
        let r = c
            .post("http://toy.test/items")
            .body("x")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 201);
        let e = fx.last_tool_call().await;
        assert_eq!(e.payload["writes"], json!([]));
    })
}

#[test]
fn a_paused_branch_holds_requests_until_it_resumes() {
    run(async {
        let fx = ProxyFx::new("paused", &upstream().await, &[]).await;
        let c = fx.client("planner");
        assert_eq!(fx.on("pause").await.0, 200);
        let held = tokio::spawn({
            let c = c.clone();
            async move { get_text(&c, "http://toy.test/hello").await }
        });
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!held.is_finished());
        assert_eq!(fx.on("resume").await.0, 200);
        assert_eq!(held.await.unwrap().0, 200);
        let (status, v) = fx.on("finish").await;
        assert_eq!(status, 200, "{v}");
        let got = tokio::time::timeout(
            Duration::from_secs(10),
            get_text(&c, "http://toy.test/hello"),
        );
        // M2: the branch's tickets died with it, and its proxy says so (not a 407 asking for a
        // ticket the client already sent).
        let (status, text) = got.await.unwrap();
        assert_eq!(status, 410, "{text}");
        assert!(text.contains("has ended"), "{text}");
    })
}

#[test]
fn a_part_that_does_not_answer_is_a_502_step() {
    run(async {
        let closed = format!("http://127.0.0.1:{}", closed_port());
        let fx = ProxyFx::new("closed", &closed, &[]).await;
        let (status, text) = get_text(&fx.client("planner"), "http://toy.test/hello").await;
        assert_eq!(status, 502);
        assert!(
            text.starts_with("the box part did not answer at toy.test"),
            "{text}"
        );
        let e = fx.last_tool_call().await;
        assert_eq!(e.payload["error"], json!(text));
        assert_eq!(e.payload["part"], "box");
    })
}

#[test]
fn a_part_that_hangs_is_a_504_step_and_frees_the_branch() {
    run(async {
        let route = format!("toy.test={}", upstream().await);
        let quick = |c: &mut shaked::app::Config| c.proxy_timeout = Duration::from_millis(500);
        let fx = ProxyFx::configured("hang", &["--http-route", &route], quick).await;
        let c = fx.client("planner");
        let hung =
            tokio::time::timeout(Duration::from_secs(5), get_text(&c, "http://toy.test/hang"));
        let (status, text) = hung.await.expect("the proxy answers in time");
        assert_eq!(status, 504);
        assert_eq!(text, "the box part did not answer at toy.test within 0.5 s");
        let e = fx.last_tool_call().await;
        assert_eq!(e.payload["error"], json!(text));
        let next = tokio::time::timeout(
            Duration::from_secs(5),
            get_text(&c, "http://toy.test/hello"),
        );
        assert_eq!(next.await.unwrap().0, 200, "the gate is free again");
    })
}

#[test]
fn after_a_save_requests_go_where_the_part_now_serves() {
    run(async {
        let (before, after) = (
            named_upstream("before").await,
            named_upstream("after").await,
        );
        let args = ["--save-at", "points", "--route-after-save", after.as_str()];
        let fx = ProxyFx::new("moved", &before, &args).await;
        let c = fx.client("planner");
        assert_eq!(get_text(&c, "http://toy.test/name").await.1, "before");
        let r = c.post("http://toy.test/items").send().await.unwrap();
        assert_eq!(r.status().as_u16(), 201);
        let (status, v) = fx.on("save").await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(get_text(&c, "http://toy.test/name").await.1, "after");
    })
}

#[test]
fn a_branch_s_tickets_are_forgotten_when_it_ends() {
    run(async {
        let fx = ProxyFx::new("forget", &upstream().await, &[]).await;
        fx.ticket(Some("planner"));
        let tickets = &fx.fx.state.proxy.tickets;
        assert!(!tickets.is_empty());
        assert_eq!(fx.on("finish").await.0, 200);
        for _ in 0..100 {
            if fx.fx.state.live.get(&fx.branch).is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(
            fx.fx.state.live.get(&fx.branch).is_none(),
            "the branch ended"
        );
        assert_eq!(tickets.len(), 0, "its tickets are forgotten");
    })
}

#[test]
fn the_ticket_is_the_branchs_and_the_agents() {
    run(async {
        let fx = ProxyFx::new("ticket", &upstream().await, &[]).await;
        let t = fx.ticket(Some("planner"));
        assert_eq!(t.len(), 32);
        let holder = fx.fx.state.proxy.tickets.holder(&t).unwrap();
        assert_eq!(
            (holder.branch.as_str(), holder.agent.as_deref()),
            (fx.branch.as_str(), Some("planner"))
        );
        assert!(fx.fx.state.proxy.tickets.holder("nope").is_none());
    })
}

#[test]
fn an_agents_register_answer_has_its_proxy_only_in_a_world_that_serves_http() {
    run(async {
        let routed = ProxyFx::new(
            "register",
            &upstream().await,
            &["--route-env", "TOY_URL=https://toy.test"],
        )
        .await;
        let a = routed
            .join(json!({"id": "planner", "name": "Planner"}))
            .await;
        let proxy = a["proxy_url"].as_str().unwrap();
        assert!(proxy.starts_with("http://shakedown:"), "{a}");
        assert_eq!(
            (&a["env"]["HTTPS_PROXY"], &a["env"]["TOY_URL"]),
            (&json!(proxy), &json!("https://toy.test"))
        );
        let plain = ProxyFx::with_part("registerplain", &[]).await;
        let a = plain
            .join(json!({"id": "planner", "name": "Planner"}))
            .await;
        // No proxy variables: only the ones that point its OTel SDK at the run.
        let env = a["env"].as_object().cloned().unwrap_or_default();
        assert!(a.get("proxy_url").is_none(), "{a}");
        assert!(env.keys().all(|k| k.starts_with("OTEL_")), "{a}");
    })
}

#[test]
fn a_driver_ticket_request_is_the_latest_model_callers() {
    run(async {
        let fx = ProxyFx::new("attributed", &upstream().await, &[]).await;
        let url = fx.fx.state.proxy.url(&fx.ticket(None)).unwrap();
        let driver = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(url).unwrap())
            .build()
            .unwrap();
        assert_eq!(get_text(&driver, "http://toy.test/hello").await.0, 200);
        let e = fx.last_tool_call().await;
        assert!(
            e.agent.is_none() && e.payload["args"].get("attributed").is_none(),
            "{e:?}"
        );
        let a = fx.join(json!({"id": "planner", "name": "Planner"})).await;
        let chat = format!("{}/chat/completions", a["gateway_url"].as_str().unwrap());
        let body = json!({"model": "m", "messages": [{"role": "user", "content": "hi"}]});
        let token = a["token"].as_str().unwrap();
        fx.fx
            .http
            .post(chat)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(get_text(&driver, "http://toy.test/hello").await.0, 200);
        let e = fx.last_tool_call().await;
        assert_eq!(e.agent.as_deref(), Some("planner"));
        assert_eq!(e.payload["args"]["attributed"], "latest model call");
    })
}

/// A client through the proxy as `planner` that trusts the world's CA (from its register answer's
/// `SSL_CERT_FILE`), and the answer's env.
async fn tls_client(fx: &ProxyFx) -> (reqwest::Client, Value) {
    let env = fx.join(json!({"id": "planner", "name": "Planner"})).await["env"].clone();
    let pem = std::fs::read(env["SSL_CERT_FILE"].as_str().unwrap()).unwrap();
    let ca = reqwest::Certificate::from_pem(&pem).unwrap();
    let proxy = reqwest::Proxy::all(env["HTTPS_PROXY"].as_str().unwrap()).unwrap();
    let c = reqwest::Client::builder()
        .proxy(proxy)
        .add_root_certificate(ca);
    (c.build().unwrap(), env)
}

#[test]
fn an_https_request_through_the_proxy_is_recorded_like_plain_http() {
    run(async {
        let fx = ProxyFx::new("https", &upstream().await, &[]).await;
        let (c, _) = tls_client(&fx).await;
        let r = c.get("https://toy.test/hello").send().await.unwrap();
        assert_eq!(r.text().await.unwrap(), "hi");
        let e = fx.last_tool_call().await;
        let step = (
            &e.payload["args"]["url"],
            &e.payload["part"],
            e.agent.as_deref(),
        );
        assert_eq!(
            step,
            (
                &json!("https://toy.test/hello"),
                &json!("box"),
                Some("planner")
            )
        );
    })
}

#[test]
fn an_https_post_saves_its_part_and_every_ca_variable_names_one_pem() {
    run(async {
        let fx = ProxyFx::new("httpspost", &upstream().await, &[]).await;
        let (c, env) = tls_client(&fx).await;
        let vars = [
            "REQUESTS_CA_BUNDLE",
            "CURL_CA_BUNDLE",
            "NODE_EXTRA_CA_CERTS",
            "GIT_SSL_CAINFO",
        ];
        for k in vars {
            assert_eq!(env[k], env["SSL_CERT_FILE"], "{k}");
        }
        let r = c
            .post("https://toy.test/items")
            .body("x")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 201);
        assert!(fx.last_tool_call().await.payload["saves"]["box"].is_string());
    })
}

#[test]
fn https_forwarding_says_https_on_443() {
    run(async {
        let fx = ProxyFx::new("httpshost", &upstream().await, &[]).await;
        let (c, _) = tls_client(&fx).await;
        let v: Value = c
            .get("https://toy.test/echo-host")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let want = json!({"host": "toy.test", "x-forwarded-host": "toy.test",
                          "x-forwarded-proto": "https", "x-forwarded-port": "443",
                          "x-shakedown-agent": "planner"});
        assert_eq!(v, want);
    })
}

#[test]
fn a_client_that_does_not_trust_the_world_ca_fails_tls() {
    run(async {
        let fx = ProxyFx::new("untrusted", &upstream().await, &[]).await;
        let before = fx.events().await.len();
        assert!(fx
            .client("planner")
            .get("https://toy.test/hello")
            .send()
            .await
            .is_err());
        let evs = fx.events().await;
        assert!(evs[before..].iter().all(|e| e.kind != EventKind::ToolCall));
    })
}

#[test]
fn connect_to_an_unknown_host_is_refused_and_recorded() {
    run(async {
        let fx = ProxyFx::new("connectunknown", &upstream().await, &[]).await;
        let (c, _) = tls_client(&fx).await;
        assert!(c.get("https://other.test/").send().await.is_err());
        let e = fx.last_tool_call().await;
        assert_eq!(e.payload["args"]["method"], "CONNECT");
        assert_eq!(e.payload["args"]["url"], "https://other.test:443");
        assert_eq!(
            e.payload["error"],
            "the host other.test is not part of this world"
        );
    })
}

#[test]
fn connect_on_another_port_is_refused() {
    run(async {
        let fx = ProxyFx::new("connectport", &upstream().await, &[]).await;
        let (c, _) = tls_client(&fx).await;
        assert!(c.get("https://toy.test:8443/hello").send().await.is_err());
        let e = fx.last_tool_call().await;
        assert_eq!(e.payload["args"]["url"], "https://toy.test:8443");
        assert_eq!(
            e.payload["error"],
            "the host toy.test takes HTTPS on port 443 only"
        );
    })
}

/// An hour from now: a ticket's expiry in these tests.
fn later() -> std::time::Instant {
    std::time::Instant::now() + std::time::Duration::from_secs(3600)
}
