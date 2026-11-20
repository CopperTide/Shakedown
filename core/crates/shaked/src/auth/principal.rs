//! The principals: a signed-in user, a service account, an API token acting for one of them, and a
//! run token bound to one branch. Only the auth layer builds them.

use uuid::Uuid;

/// A member's role in an org.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrgRole {
    Owner,
    Admin,
    Member,
    Viewer,
    Billing,
}

impl OrgRole {
    /// The name `identity.memberships.role` and `app.org_role` use.
    pub fn as_str(self) -> &'static str {
        match self {
            OrgRole::Owner => "owner",
            OrgRole::Admin => "admin",
            OrgRole::Member => "member",
            OrgRole::Viewer => "viewer",
            OrgRole::Billing => "billing",
        }
    }

    pub fn parse(s: &str) -> Option<OrgRole> {
        [
            OrgRole::Owner,
            OrgRole::Admin,
            OrgRole::Member,
            OrgRole::Viewer,
            OrgRole::Billing,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
    }
}

/// A seat on a team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeatRole {
    Maintainer,
    Member,
}

/// What a principal may do with one resource, lowest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    None,
    View,
    Run,
    Edit,
    Admin,
}

impl Level {
    /// A grant's level as `shakedown.grants.level` names it.
    pub fn parse(s: &str) -> Option<Level> {
        match s {
            "view" => Some(Level::View),
            "run" => Some(Level::Run),
            "edit" => Some(Level::Edit),
            "admin" => Some(Level::Admin),
            _ => None,
        }
    }
}

/// An API token's scope: `read` views, `run` also starts, forks, stops and records, `admin`
/// does everything its holder may.
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

    /// The highest level this scope allows.
    pub fn cap(self) -> Level {
        match self {
            TokenScope::Read => Level::View,
            TokenScope::Run => Level::Edit,
            TokenScope::Admin => Level::Admin,
        }
    }
}

/// What a run token is for (`shakedown.run_tokens.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunTokenKind {
    /// Register agents, report, finish.
    Run,
    /// One agent's MCP and model gateway calls.
    Agent,
    /// A sandbox's link for one branch.
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

/// Who is calling.
#[derive(Debug, Clone, PartialEq)]
pub enum Principal {
    /// A person, in the org they act in, with their seats there.
    User {
        user: Uuid,
        org: Uuid,
        role: OrgRole,
        seats: Vec<(Uuid, SeatRole)>,
    },
    /// A non-person principal of an org; bound to `team`, it acts as a member of that team only.
    ServiceAccount {
        sa: Uuid,
        org: Uuid,
        role: OrgRole,
        team: Option<Uuid>,
    },
    /// An API token acting as `holder`, capped by its scopes and, when set, to `team`.
    ApiToken {
        token: Uuid,
        holder: Box<Principal>,
        scopes: Vec<TokenScope>,
        team: Option<Uuid>,
    },
    /// A machine token bound to one branch (and to an agent or a sandbox).
    RunToken {
        org: Uuid,
        run: String,
        branch: String,
        kind: RunTokenKind,
        agent: Option<String>,
        sandbox: Option<Uuid>,
    },
}

impl Principal {
    /// The org this principal acts in.
    pub fn org(&self) -> Uuid {
        match self {
            Principal::User { org, .. }
            | Principal::ServiceAccount { org, .. }
            | Principal::RunToken { org, .. } => *org,
            Principal::ApiToken { holder, .. } => holder.org(),
        }
    }
}
