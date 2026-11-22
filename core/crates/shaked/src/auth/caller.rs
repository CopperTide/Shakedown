//! Who calls: the person at this machine, an API token, or a run token bound to one branch. A
//! hosting layer may put its own principal in `ext`; the engine never reads it.

use axum::http::Extensions;
use uuid::Uuid;

/// The caller of a request or a background step.
#[derive(Debug, Clone)]
pub struct Caller {
    pub kind: CallerKind,
    /// What a hosting layer knows about the caller (its principal); empty locally.
    pub ext: Extensions,
}

/// The engine's view of a caller.
#[derive(Debug, Clone, PartialEq)]
pub enum CallerKind {
    /// Locally: whoever sits at this machine.
    Person,
    /// An `sd_` token, capped by its scopes.
    ApiToken { id: Uuid, scopes: Vec<TokenScope> },
    /// A machine token bound to one branch (and to an agent, or a sandbox for worker and ticket).
    RunToken {
        run: String,
        branch: String,
        kind: RunTokenKind,
        agent: Option<String>,
        sandbox: Option<Uuid>,
    },
}

impl Caller {
    fn of(kind: CallerKind) -> Caller {
        Caller {
            kind,
            ext: Extensions::new(),
        }
    }

    pub fn person() -> Caller {
        Caller::of(CallerKind::Person)
    }

    pub fn api_token(id: Uuid, scopes: Vec<TokenScope>) -> Caller {
        Caller::of(CallerKind::ApiToken { id, scopes })
    }

    pub fn run_token(run: &str, branch: &str, kind: RunTokenKind, agent: Option<&str>) -> Caller {
        Caller::of(CallerKind::RunToken {
            run: run.to_string(),
            branch: branch.to_string(),
            kind,
            agent: agent.map(str::to_string),
            sandbox: None,
        })
    }

    /// `run_token`, for a worker or ticket token of `sandbox`.
    pub fn with_sandbox(mut self, id: Uuid) -> Caller {
        if let CallerKind::RunToken { sandbox, .. } = &mut self.kind {
            *sandbox = Some(id);
        }
        self
    }
}

/// An API token's scope: `read` views, `run` also starts, forks, stops and records, `admin`
/// does everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenScope {
    Read,
    Run,
    Admin,
}

impl TokenScope {
    pub fn as_str(self) -> &'static str {
        match self {
            TokenScope::Read => "read",
            TokenScope::Run => "run",
            TokenScope::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Option<TokenScope> {
        [TokenScope::Read, TokenScope::Run, TokenScope::Admin]
            .into_iter()
            .find(|t| t.as_str() == s)
    }
}

/// What a run token is for (`shakedown.run_tokens.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunTokenKind {
    /// Register agents, report, finish.
    Run,
    /// One agent's MCP and model gateway calls.
    Agent,
    /// A sandbox's link for one branch, and its agents' model calls.
    Worker,
    /// One-time, traded for a worker token.
    Ticket,
}

impl RunTokenKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RunTokenKind::Run => "run",
            RunTokenKind::Agent => "agent",
            RunTokenKind::Worker => "worker",
            RunTokenKind::Ticket => "ticket",
        }
    }
}
