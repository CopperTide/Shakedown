//! The E2B backend. The control box calls E2B's API with its key (`X-API-KEY`) to create, look
//! at and kill sandboxes, and each sandbox's envd (port 49983 on the sandbox's own host) with
//! that sandbox's access token (`X-Access-Token`) to write files into it. The E2B key goes only
//! to the API, never to a sandbox. The access tokens are kept in memory only (Task 8 keeps them
//! sealed with the sandbox's row).
//!
//! Endpoints (E2B's `spec/openapi.yml` and `spec/envd/envd.yaml`):
//!
//! | Call | Request | Answer |
//! |---|---|---|
//! | create | `POST {api}/sandboxes` | 201 `{sandboxID, envdAccessToken, domain}` |
//! | state | `GET {api}/sandboxes/{id}` | 200 `{state: running \| paused}`, 404 gone |
//! | kill | `DELETE {api}/sandboxes/{id}` | 204, 404 already gone |
//! | write | `POST https://49983-{id}.{domain}/files?path=…&username=root`, multipart `file` | 200 |

use std::{collections::HashMap, sync::Mutex, time::Duration};

use futures_util::future::BoxFuture;
use reqwest::{header::CONTENT_TYPE, Response, StatusCode, Url};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{ProviderRef, ProviderState, SandboxBackend, SandboxSpec};
use crate::providers::Secret;

/// E2B's API.
pub const API: &str = "https://api.e2b.app";
/// Where sandboxes answer when E2B names no domain.
const DOMAIN: &str = "e2b.app";
const ENVD_PORT: u16 = 49983;
/// Files are written as root: the ticket's folder is the system's.
const FILE_USER: &str = "root";

/// Sandboxes on E2B.
pub struct E2bBackend {
    key: Secret,
    api: Url,
    /// Every sandbox's envd at this base instead of its own host (tests).
    envd_at: Option<String>,
    http: reqwest::Client,
    envd: Mutex<HashMap<String, Envd>>,
}

/// How to reach one sandbox's envd.
#[derive(Clone)]
struct Envd {
    base: String,
    token: Secret,
}

/// What E2B answers a create with.
#[derive(Deserialize)]
struct Created {
    #[serde(rename = "sandboxID")]
    sandbox_id: String,
    #[serde(rename = "envdAccessToken")]
    envd_access_token: String,
    domain: Option<String>,
}

impl E2bBackend {
    /// `api` is E2B's API base, [`API`].
    pub fn new(api_key: String, api: Url) -> E2bBackend {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .expect("http client");
        E2bBackend {
            key: Secret::new(api_key),
            api,
            envd_at: None,
            http,
            envd: Mutex::default(),
        }
    }

    /// Sends every envd call to `base` instead of the sandbox's host. For tests.
    pub fn with_envd(self, base: Url) -> E2bBackend {
        let envd_at = Some(base.as_str().trim_end_matches('/').to_string());
        E2bBackend { envd_at, ..self }
    }

    /// Reads a file back from a sandbox (live checks).
    pub async fn read_file(&self, sb: &ProviderRef, path: &str) -> anyhow::Result<Vec<u8>> {
        let (url, token) = self.files(sb, path)?;
        let res = self.http.get(url).header("X-Access-Token", token.expose());
        let res = answered(res.send().await?, "read a file").await?;
        Ok(res.bytes().await?.to_vec())
    }

    /// `{api}/sandboxes`, then `id` when given.
    fn sandboxes(&self, id: Option<&str>) -> Url {
        let mut url = self.api.clone();
        url.path_segments_mut()
            .expect("E2B's API is an http(s) URL")
            .pop_if_empty()
            .push("sandboxes")
            .extend(id);
        url
    }

    /// The envd files URL for `path` in `sb`, and the token it takes.
    fn files(&self, sb: &ProviderRef, path: &str) -> anyhow::Result<(Url, Secret)> {
        let envd = self.envd.lock().unwrap().get(&sb.0).cloned();
        let Some(envd) = envd else {
            anyhow::bail!(
                "no envd access token for sandbox {}: this control box did not create it",
                sb.0
            );
        };
        let mut url = Url::parse(&format!("{}/files", envd.base))?;
        url.query_pairs_mut()
            .append_pair("path", path)
            .append_pair("username", FILE_USER);
        Ok((url, envd.token))
    }

    fn keep(&self, made: Created) -> ProviderRef {
        let domain = made.domain.as_deref().unwrap_or(DOMAIN);
        let base = self
            .envd_at
            .clone()
            .unwrap_or_else(|| format!("https://{ENVD_PORT}-{}.{domain}", made.sandbox_id));
        let token = Secret::new(made.envd_access_token);
        let envd = Envd { base, token };
        self.envd
            .lock()
            .unwrap()
            .insert(made.sandbox_id.clone(), envd);
        ProviderRef(made.sandbox_id)
    }

    fn api_call(&self, method: reqwest::Method, id: Option<&str>) -> reqwest::RequestBuilder {
        self.http
            .request(method, self.sandboxes(id))
            .header("X-API-KEY", self.key.expose())
    }
}

/// The create body: the template, the timeout, and egress only to `allow_out`.
fn create_body(spec: &SandboxSpec) -> Value {
    json!({
        "templateID": spec.template,
        "timeout": spec.timeout.as_secs(),
        "secure": true,
        "metadata": spec.metadata,
        "network": {
            "allowOut": spec.allow_out,
            "denyOut": ["0.0.0.0/0"],
            "allowPublicTraffic": false,
        },
    })
}

/// A multipart body with `bytes` as its one `file` field, and its content type.
fn multipart(bytes: Vec<u8>) -> (String, Vec<u8>) {
    let boundary = format!("shaked-{}", Uuid::new_v4().simple());
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"file\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend(bytes);
    body.extend(format!("\r\n--{boundary}--\r\n").into_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// `res` when it succeeded, else an error naming what failed, with E2B's status and message.
async fn answered(res: Response, what: &str) -> anyhow::Result<Response> {
    if res.status().is_success() {
        return Ok(res);
    }
    let status = res.status();
    let body: String = res
        .text()
        .await
        .unwrap_or_default()
        .chars()
        .take(500)
        .collect();
    anyhow::bail!("E2B could not {what}: {status} {body}")
}

impl SandboxBackend for E2bBackend {
    fn create<'a>(&'a self, spec: &'a SandboxSpec) -> BoxFuture<'a, anyhow::Result<ProviderRef>> {
        Box::pin(async move {
            let req = self
                .api_call(reqwest::Method::POST, None)
                .json(&create_body(spec));
            let made: Created = answered(req.send().await?, "create a sandbox")
                .await?
                .json()
                .await?;
            Ok(self.keep(made))
        })
    }

    fn write_file<'a>(
        &'a self,
        sb: &'a ProviderRef,
        path: &'a str,
        bytes: Vec<u8>,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move {
            let (url, token) = self.files(sb, path)?;
            let (kind, body) = multipart(bytes);
            let req = self.http.post(url).header("X-Access-Token", token.expose());
            let req = req.header(CONTENT_TYPE, kind).body(body);
            answered(req.send().await?, "write a file").await?;
            Ok(())
        })
    }

    fn kill<'a>(&'a self, sb: &'a ProviderRef) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move {
            let res = self
                .api_call(reqwest::Method::DELETE, Some(&sb.0))
                .send()
                .await?;
            if res.status() != StatusCode::NOT_FOUND {
                answered(res, "kill a sandbox").await?;
            }
            self.envd.lock().unwrap().remove(&sb.0);
            Ok(())
        })
    }

    fn state<'a>(&'a self, sb: &'a ProviderRef) -> BoxFuture<'a, anyhow::Result<ProviderState>> {
        Box::pin(async move {
            let res = self
                .api_call(reqwest::Method::GET, Some(&sb.0))
                .send()
                .await?;
            if res.status() == StatusCode::NOT_FOUND {
                return Ok(ProviderState::Gone);
            }
            let detail: Value = answered(res, "look at a sandbox").await?.json().await?;
            Ok(match detail["state"].as_str() {
                Some("running") => ProviderState::Running,
                _ => ProviderState::Gone,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
        time::Duration,
    };

    use axum::{
        body::Bytes,
        http::{HeaderMap, Method, StatusCode, Uri},
        Json, Router,
    };
    use serde_json::{json, Value};

    use super::*;
    use crate::hosting::TICKET_PATH;

    const KEY: &str = "e2b_test_key_1234";

    /// A request the mock got.
    struct Seen {
        method: Method,
        uri: Uri,
        headers: HeaderMap,
        body: Bytes,
    }

    type Answers = HashMap<(Method, &'static str), (u16, Value)>;

    /// A mock of E2B's API and envd on one port. It answers each `(method, path)` it was given and
    /// 404s the rest, keeping every request.
    async fn mock(answers: Answers) -> (Url, Arc<Mutex<Vec<Seen>>>) {
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let keep = seen.clone();
        let app = Router::new().fallback(
            move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
                let found = answers.get(&(method.clone(), uri.path())).cloned();
                keep.lock().unwrap().push(Seen {
                    method,
                    uri,
                    headers,
                    body,
                });
                let (status, body) =
                    found.unwrap_or((404, json!({"code": 404, "message": "not found"})));
                async move { (StatusCode::from_u16(status).unwrap(), Json(body)) }
            },
        );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base: Url = format!("http://{}", l.local_addr().unwrap())
            .parse()
            .unwrap();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        (base, seen)
    }

    fn backend(base: &Url) -> E2bBackend {
        E2bBackend::new(KEY.into(), base.clone()).with_envd(base.clone())
    }

    fn created() -> Answers {
        let made = json!({"sandboxID": "sbx1", "templateID": "tpl", "clientID": "c",
                          "envdVersion": "0.2.0", "envdAccessToken": "envd-tok-1", "domain": null});
        HashMap::from([((Method::POST, "/sandboxes"), (201, made))])
    }

    fn spec() -> SandboxSpec {
        SandboxSpec {
            template: "shakedown-worker-abc".into(),
            allow_out: vec!["sd.coppertide.ai".into(), "api.anthropic.com".into()],
            timeout: Duration::from_secs(65 * 60),
            metadata: BTreeMap::from([("run".into(), "r1".into()), ("branch".into(), "b1".into())]),
        }
    }

    #[tokio::test]
    async fn create_sends_the_allowlist_and_secure() {
        let (base, seen) = mock(created()).await;
        let sb = backend(&base).create(&spec()).await.unwrap();
        assert_eq!(sb, ProviderRef("sbx1".into()));
        let seen = seen.lock().unwrap();
        let (req, body) = (
            &seen[0],
            serde_json::from_slice::<Value>(&seen[0].body).unwrap(),
        );
        assert_eq!((&req.method, req.uri.path()), (&Method::POST, "/sandboxes"));
        assert_eq!(req.headers["x-api-key"], KEY);
        assert_eq!(
            body,
            json!({"templateID": "shakedown-worker-abc", "timeout": 3900, "secure": true,
                   "metadata": {"run": "r1", "branch": "b1"},
                   "network": {"allowOut": ["sd.coppertide.ai", "api.anthropic.com"],
                               "denyOut": ["0.0.0.0/0"], "allowPublicTraffic": false}})
        );
    }

    #[tokio::test]
    async fn kill_404_is_done() {
        let failing = HashMap::from([((Method::DELETE, "/sandboxes/bad"), (500, json!({})))]);
        let (base, seen) = mock(failing).await;
        let e2b = backend(&base);
        e2b.kill(&ProviderRef("gone1".into())).await.unwrap();
        assert!(e2b.kill(&ProviderRef("bad".into())).await.is_err());
        let seen = seen.lock().unwrap();
        assert_eq!(
            (&seen[0].method, seen[0].uri.path()),
            (&Method::DELETE, "/sandboxes/gone1")
        );
        assert_eq!(seen[0].headers["x-api-key"], KEY);
    }

    #[tokio::test]
    async fn state_404_is_gone() {
        let running = HashMap::from([(
            (Method::GET, "/sandboxes/up1"),
            (200, json!({"state": "running"})),
        )]);
        let (base, seen) = mock(running).await;
        let e2b = backend(&base);
        let gone = e2b.state(&ProviderRef("gone1".into())).await.unwrap();
        let up = e2b.state(&ProviderRef("up1".into())).await.unwrap();
        assert_eq!((gone, up), (ProviderState::Gone, ProviderState::Running));
        assert_eq!(seen.lock().unwrap()[0].headers["x-api-key"], KEY);
    }

    /// `req` carries `bytes` as its one multipart `file` field.
    fn assert_multipart(req: &Seen, bytes: &str) {
        let kind = req.headers["content-type"].to_str().unwrap();
        assert!(kind.starts_with("multipart/form-data; boundary="), "{kind}");
        let body = String::from_utf8_lossy(&req.body);
        let field = format!("\r\n\r\n{bytes}\r\n--");
        assert!(
            body.contains("name=\"file\"") && body.contains(&field),
            "{body}"
        );
    }

    #[tokio::test]
    async fn write_file_sends_the_access_token() {
        let mut answers = created();
        let wrote = (200, json!([{"path": TICKET_PATH}]));
        answers.insert((Method::POST, "/files"), wrote);
        let (base, seen) = mock(answers).await;
        let e2b = backend(&base);
        let sb = e2b.create(&spec()).await.unwrap();
        let ticket = "sdt_abc\n";
        e2b.write_file(&sb, TICKET_PATH, ticket.into())
            .await
            .unwrap();
        let unknown = ProviderRef("other".into());
        assert!(e2b.write_file(&unknown, TICKET_PATH, vec![]).await.is_err());
        let seen = seen.lock().unwrap();
        let req = &seen[1];
        assert_eq!((&req.method, req.uri.path()), (&Method::POST, "/files"));
        let query = Some("path=%2Frun%2Fshaked%2Fticket&username=root");
        assert_eq!(req.uri.query(), query);
        assert_eq!(req.headers["x-access-token"], "envd-tok-1");
        let no_key = req.headers.get("x-api-key").is_none();
        assert!(no_key, "the E2B key never goes to a sandbox");
        assert_multipart(req, ticket);
    }
}
