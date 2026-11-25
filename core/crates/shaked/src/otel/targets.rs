//! The one-click targets: what Settings → Observability asks for each preset, worked out into an
//! OTLP/HTTP traces endpoint and its headers.
//!
//! | preset   | endpoint                                             | headers                         |
//! |----------|------------------------------------------------------|---------------------------------|
//! | Langfuse | `<host>/api/public/otel/v1/traces` (US, EU or yours) | `Authorization: Basic pk:sk`    |
//! | Phoenix  | `<url>/v1/traces` (yours, or a Phoenix Cloud space)  | `Authorization: Bearer`, `api_key` |
//! | OTLP     | the URL given, `/v1/traces` added unless there       | as given                        |

use std::collections::BTreeMap;

use base64::Engine;
use serde::Deserialize;

use crate::store::{TargetFlags, TargetKind};

pub const LANGFUSE_US: &str = "https://us.cloud.langfuse.com";
pub const LANGFUSE_EU: &str = "https://cloud.langfuse.com";
/// Arize Phoenix's own address when it runs on this machine.
pub const PHOENIX_LOCAL: &str = "http://127.0.0.1:6006";
const TRACES: &str = "/v1/traces";
const MAX_HEADERS: usize = 20;
const MAX_HEADER: usize = 4096;

/// What the form sends for a new target.
#[derive(Debug, Default, Deserialize)]
pub struct Asked {
    pub name: String,
    pub kind: String,
    /// Langfuse: `us`, `eu` or `self` (then `base_url`).
    #[serde(default)]
    pub region: Option<String>,
    /// Langfuse or Phoenix on your own server; a Phoenix Cloud space's URL.
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub secret_key: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    /// Phoenix's project (`shakedown` when blank).
    #[serde(default)]
    pub project: Option<String>,
    /// Generic OTLP: the collector's URL.
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub include_content: Option<bool>,
    #[serde(default)]
    pub auto_export: Option<bool>,
    #[serde(default)]
    pub export_metrics: Option<bool>,
}

/// A target worked out from what was asked.
#[derive(Debug, PartialEq)]
pub struct Resolved {
    pub kind: TargetKind,
    pub endpoint: String,
    pub project: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub flags: TargetFlags,
}

fn given(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn needed(s: &Option<String>, what: &str) -> Result<String, String> {
    given(s).ok_or_else(|| format!("{what} is required"))
}

/// `url` with `/v1/traces` at the end of its path (once).
pub fn traces_url(url: &str) -> String {
    let base = url.trim().trim_end_matches('/');
    match base.ends_with(TRACES) {
        true => base.to_string(),
        false => format!("{base}{TRACES}"),
    }
}

/// The metrics URL next to traces URL `url`.
pub fn metrics_url(url: &str) -> String {
    let base = url.strip_suffix(TRACES).unwrap_or(url);
    format!("{base}/v1/metrics")
}

fn langfuse(a: &Asked) -> Result<(String, BTreeMap<String, String>, bool), String> {
    let (base, hosted) = match given(&a.region).as_deref().unwrap_or("us") {
        "us" => (LANGFUSE_US.to_string(), true),
        "eu" => (LANGFUSE_EU.to_string(), true),
        "self" => (needed(&a.base_url, "your Langfuse's URL")?, false),
        other => return Err(format!("unknown Langfuse region {other:?}: us, eu or self")),
    };
    let pk = needed(&a.public_key, "the public key (pk-lf-…)")?;
    let sk = needed(&a.secret_key, "the secret key (sk-lf-…)")?;
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("{pk}:{sk}"));
    let headers = BTreeMap::from([("Authorization".to_string(), format!("Basic {basic}"))]);
    let endpoint = format!("{}/api/public/otel{TRACES}", base.trim_end_matches('/'));
    Ok((endpoint, headers, hosted))
}

fn phoenix(a: &Asked) -> (String, BTreeMap<String, String>, bool) {
    let base = given(&a.base_url).unwrap_or_else(|| PHOENIX_LOCAL.to_string());
    let hosted = reqwest::Url::parse(&base)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.ends_with("phoenix.arize.com")))
        .unwrap_or(false);
    let mut headers = BTreeMap::new();
    if let Some(k) = given(&a.api_key) {
        headers.insert("Authorization".to_string(), format!("Bearer {k}"));
        headers.insert("api_key".to_string(), k);
    }
    (traces_url(&base), headers, hosted)
}

/// The headers given, checked: at most 20, each a valid header of at most 4 KiB.
fn checked_headers(h: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>, String> {
    if h.len() > MAX_HEADERS {
        return Err(format!("at most {MAX_HEADERS} headers"));
    }
    let mut out = BTreeMap::new();
    for (k, v) in h {
        let (k, v) = (k.trim(), v.trim());
        let name_ok = reqwest::header::HeaderName::from_bytes(k.as_bytes()).is_ok();
        let value_ok = reqwest::header::HeaderValue::from_str(v).is_ok();
        if !name_ok || !value_ok || v.len() > MAX_HEADER {
            return Err(format!("header {k:?} is not a valid HTTP header"));
        }
        out.insert(k.to_string(), v.to_string());
    }
    Ok(out)
}

/// The target `a` asks for. Content goes by default only to a target on your own server (a
/// cloud service gets numbers and shapes unless asked for content).
pub fn resolve(a: &Asked) -> Result<Resolved, String> {
    let kind = TargetKind::parse(a.kind.trim())
        .ok_or_else(|| format!("unknown kind {:?}: langfuse, phoenix or otlp", a.kind))?;
    let (endpoint, headers, hosted) = match kind {
        TargetKind::Langfuse => langfuse(a)?,
        TargetKind::Phoenix => phoenix(a),
        TargetKind::Otlp => {
            let url = needed(&a.endpoint, "the collector's URL")?;
            (traces_url(&url), checked_headers(&a.headers)?, false)
        }
    };
    let project = match kind {
        TargetKind::Phoenix => Some(given(&a.project).unwrap_or_else(|| "shakedown".into())),
        _ => None,
    };
    let flags = TargetFlags {
        include_content: a.include_content.unwrap_or(!hosted),
        auto_export: a.auto_export.unwrap_or(true),
        export_metrics: kind == TargetKind::Otlp && a.export_metrics.unwrap_or(false),
    };
    Ok(Resolved {
        kind,
        endpoint,
        project,
        headers,
        flags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(kind: &str) -> Asked {
        Asked {
            name: "t".into(),
            kind: kind.into(),
            ..Asked::default()
        }
    }

    #[test]
    fn langfuse_cloud_is_basic_auth_on_its_otel_endpoint_without_content() {
        let a = Asked {
            region: Some("eu".into()),
            public_key: Some("pk-lf-1".into()),
            secret_key: Some("sk-lf-2".into()),
            ..asked("langfuse")
        };
        let r = resolve(&a).unwrap();
        assert_eq!(
            r.endpoint,
            "https://cloud.langfuse.com/api/public/otel/v1/traces"
        );
        let basic = base64::engine::general_purpose::STANDARD.encode("pk-lf-1:sk-lf-2");
        assert_eq!(r.headers["Authorization"], format!("Basic {basic}"));
        assert!(
            !r.flags.include_content,
            "a cloud service gets no content by default"
        );
        assert!(resolve(&Asked {
            secret_key: None,
            ..a
        })
        .is_err());
    }

    #[test]
    fn phoenix_on_your_machine_takes_content_and_names_its_project() {
        let r = resolve(&asked("phoenix")).unwrap();
        assert_eq!(r.endpoint, "http://127.0.0.1:6006/v1/traces");
        assert_eq!(r.project.as_deref(), Some("shakedown"));
        assert!(r.flags.include_content && r.headers.is_empty());
        let cloud = Asked {
            base_url: Some("https://app.phoenix.arize.com/s/me".into()),
            api_key: Some("k".into()),
            ..asked("phoenix")
        };
        let r = resolve(&cloud).unwrap();
        assert_eq!(r.endpoint, "https://app.phoenix.arize.com/s/me/v1/traces");
        assert_eq!(r.headers["Authorization"], "Bearer k");
        assert!(!r.flags.include_content);
    }

    #[test]
    fn generic_otlp_gets_its_traces_path_once_and_checked_headers() {
        let mut a = Asked {
            endpoint: Some("http://jaeger:4318/".into()),
            ..asked("otlp")
        };
        assert_eq!(
            resolve(&a).unwrap().endpoint,
            "http://jaeger:4318/v1/traces"
        );
        a.endpoint = Some("https://api.honeycomb.io/v1/traces".into());
        a.headers.insert("x-honeycomb-team".into(), "abc".into());
        let r = resolve(&a).unwrap();
        assert_eq!(r.endpoint, "https://api.honeycomb.io/v1/traces");
        assert_eq!(
            metrics_url(&r.endpoint),
            "https://api.honeycomb.io/v1/metrics"
        );
        a.headers.insert("bad header".into(), "x".into());
        assert!(resolve(&a).is_err());
        assert!(resolve(&asked("zipkin")).is_err());
    }
}
