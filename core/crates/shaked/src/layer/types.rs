//! What the engine and a layer pass each other at the hooks.

use axum::http::StatusCode;
use uuid::Uuid;

use crate::{
    auth::{Caller, SystemJob},
    providers::Provider,
};

pub type BoxFuture<'a, T> = futures_util::future::BoxFuture<'a, T>;

/// Static facts about a deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// It answers people other than whoever sits at this machine (a bind beyond loopback is fine).
    pub serves_others: bool,
    /// Model servers on private addresses (a vLLM box on the LAN) may be used.
    pub private_model_urls: bool,
    /// Packages and code connections run on this machine (else only in sandboxes).
    pub runs_packages_here: bool,
    /// The operator's own model keys (`OPENAI_API_KEY` and the rest, local servers) and the
    /// machine's AWS credentials (env, the aws CLI, an instance role) may serve callers. Off for a
    /// layer that serves others: each caller brings keys of their own, else the layer's fallback.
    pub environment_keys: bool,
}

/// The SHA-256 of a request's body. A layer that serves others finds it in the request's
/// extensions in `authenticate`, so a signature it checks can cover the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodySha256(pub [u8; 32]);

/// An answer that stops a request: its status and message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub status: StatusCode,
    pub message: String,
}

impl Refusal {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Refusal {
        Refusal {
            status,
            message: message.into(),
        }
    }
}

/// What a transaction is for: a caller's request, or a system job.
#[derive(Debug, Clone, Copy)]
pub enum TxFor<'a> {
    Caller(&'a Caller),
    System(SystemJob),
}

/// An operation that costs something, asked about before it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Costly<'a> {
    StartRun,
    Fork {
        branch: &'a str,
    },
    ModelCall {
        branch: &'a str,
        provider: &'a str,
        model: &'a str,
        /// The call's own `max_tokens` (or `max_completion_tokens`), if it names one.
        max_tokens: Option<u64>,
        /// The request body's length, for estimating its prompt.
        request_bytes: u64,
    },
    StoreSave {
        bytes: u64,
    },
}

/// The layer's answer to [`Costly`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    Go,
    /// Go, and tell the caller (a soft quota).
    Warn(String),
    Refuse {
        status: StatusCode,
        message: String,
    },
}

/// What usage is counted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Meter {
    Runs,
    ModelUsdMicros,
    ModelTokens,
    SavesBytes,
    SandboxSeconds,
}

/// Whose key paid for a model call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// One of the caller's own provider keys.
    Own(Uuid),
    /// A key in shaked's environment.
    Environment,
    /// The layer's fallback (its name).
    Fallback(&'static str),
}

/// Usage after the fact; `key` makes it idempotent.
#[derive(Debug, Clone, PartialEq)]
pub struct Usage<'a> {
    pub meter: Meter,
    pub quantity: i64,
    pub key: String,
    pub run: Option<&'a str>,
    pub branch: Option<&'a str>,
    pub paid_by: KeySource,
}

/// The model an agent asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelWant<'a> {
    pub provider: Option<&'a str>,
    pub model: &'a str,
}

/// A provider the layer lends when the caller has no key for what an agent asks.
#[derive(Debug, Clone, PartialEq)]
pub struct Fallback {
    pub provider: Provider,
    pub source: &'static str,
}

/// An auditable change: never a secret.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent<'a> {
    pub action: &'static str,
    pub target: (&'static str, &'a str),
    pub detail: serde_json::Value,
}

/// The layer's fields of `GET /api/me`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeExtra {
    pub mode: &'static str,
    pub features: Vec<String>,
    pub account: Option<serde_json::Value>,
}
