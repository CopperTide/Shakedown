//! `can(principal, action, target)`: the accounts spec §3, verbatim. A principal's level on a
//! resource is the highest of its org role, an org-wide resource, a team seat, authorship and
//! grants, then capped by role, token scope and team binding; org-level actions follow the
//! permission matrix; run tokens reach only the run-internal actions of their own branch.

use uuid::Uuid;

use super::principal::{Level, OrgRole, Principal, RunTokenKind, SeatRole, TokenScope};

/// What a principal asks to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // On a resource (needs a level).
    Read,
    Start,
    Fork,
    Stop,
    Record,
    UseInRun,
    Rename,
    ChangeSetup,
    ReplaceSecret,
    DeleteFork,
    Share,
    MoveTeam,
    Delete,
    // Inside a run (run tokens only).
    RegisterAgent,
    Report,
    Finish,
    CallTools,
    CallModel,
    WorkerLink,
    // On the org (the permission matrix).
    SeeOrg,
    CreateOrgWide(ResourceKind),
    CreateTeam,
    ManageSeats(Uuid),
    Invite,
    ChangeRole { target: OrgRole },
    ServiceAccounts,
    AnyTokens,
    OwnTokens,
    Audit,
    Usage,
    Billing,
    OrgSettings,
    DeleteOrg,
}

/// The kinds of root resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Run,
    Experiment,
    Suite,
    World,
    ProviderKey,
    Connection,
}

/// A root resource as `can` sees it: its org, team scope (None = org-wide), creator and grants.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub kind: ResourceKind,
    pub org: Uuid,
    pub id: String,
    pub team: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub grants: Vec<Grant>,
}

/// A share of one resource with a member or a team of its org.
#[derive(Debug, Clone, PartialEq)]
pub struct Grant {
    pub to: Grantee,
    pub level: Level,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Grantee {
    User(Uuid),
    Team(Uuid),
}

/// What an action is aimed at.
#[derive(Debug, Clone, Copy)]
pub enum Target<'a> {
    Org(Uuid),
    Resource(&'a Resource),
    Branch {
        org: Uuid,
        run: &'a str,
        branch: &'a str,
    },
}

/// The org settings `can` reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct OrgFlags {
    pub members_create_teams: bool,
}

/// The level `p` has on `r`: None for another org's resource and for run tokens.
pub fn level(p: &Principal, r: &Resource) -> Level {
    if p.org() != r.org {
        return Level::None;
    }
    match p {
        Principal::User {
            user, role, seats, ..
        } => person_level(Some(*user), *role, seats, r),
        Principal::ServiceAccount {
            role, team: None, ..
        } => person_level(None, *role, &[], r),
        Principal::ServiceAccount { team: Some(t), .. } => {
            let as_member = person_level(None, OrgRole::Member, &[(*t, SeatRole::Member)], r);
            bound(Some(*t), r, as_member)
        }
        Principal::ApiToken {
            holder,
            scopes,
            team,
            ..
        } => bound(*team, r, level(holder, r).min(scope_cap(scopes))),
        Principal::RunToken { .. } => Level::None,
    }
}

/// The highest of: an owner's or admin's role, an org-wide resource for a member or viewer, a seat
/// on the resource's team, authorship and grants; then capped by the role.
fn person_level(
    user: Option<Uuid>,
    role: OrgRole,
    seats: &[(Uuid, SeatRole)],
    r: &Resource,
) -> Level {
    let mut l = Level::None;
    if matches!(role, OrgRole::Owner | OrgRole::Admin) {
        l = Level::Admin;
    }
    if r.team.is_none() && matches!(role, OrgRole::Member | OrgRole::Viewer) {
        l = l.max(Level::Run);
    }
    for (_, seat) in seats.iter().filter(|(t, _)| Some(*t) == r.team) {
        l = l.max(match seat {
            SeatRole::Maintainer => Level::Admin,
            SeatRole::Member => Level::Edit,
        });
    }
    if user.is_some() && r.created_by == user {
        l = l.max(Level::Admin);
    }
    l = l.max(granted_level(user, seats, r));
    match role {
        OrgRole::Viewer => l.min(Level::View),
        OrgRole::Billing => Level::None,
        _ => l,
    }
}

/// The highest grant on `r` to `user` or to a team it sits on.
fn granted_level(user: Option<Uuid>, seats: &[(Uuid, SeatRole)], r: &Resource) -> Level {
    let mine = |g: &&Grant| match g.to {
        Grantee::User(u) => Some(u) == user,
        Grantee::Team(t) => seats.iter().any(|(s, _)| *s == t),
    };
    r.grants
        .iter()
        .filter(mine)
        .map(|g| g.level)
        .max()
        .unwrap_or(Level::None)
}

/// A principal bound to `team` reaches only that team's resources and org-wide ones.
fn bound(team: Option<Uuid>, r: &Resource, l: Level) -> Level {
    match (team, r.team) {
        (Some(t), Some(rt)) if t != rt => Level::None,
        _ => l,
    }
}

/// The highest level any of `scopes` allows.
fn scope_cap(scopes: &[TokenScope]) -> Level {
    scopes.iter().map(|s| s.cap()).max().unwrap_or(Level::None)
}

/// The level a resource action needs. Other actions are not decided by level; for them this is
/// `Admin`, so a mistaken level check fails closed.
pub fn required(a: &Action) -> Level {
    use Action::*;
    match a {
        Read => Level::View,
        Start | Fork | Stop | Record | UseInRun => Level::Run,
        Rename | ChangeSetup | ReplaceSecret | DeleteFork => Level::Edit,
        _ => Level::Admin,
    }
}

/// True for the actions on a resource (the spec's resource-action table).
fn on_resource(a: &Action) -> bool {
    use Action::*;
    matches!(
        a,
        Read | Start
            | Fork
            | Stop
            | Record
            | UseInRun
            | Rename
            | ChangeSetup
            | ReplaceSecret
            | DeleteFork
            | Share
            | MoveTeam
            | Delete
    )
}

/// May `p` do `a` to `t`?
pub fn can(p: &Principal, a: &Action, t: &Target<'_>, flags: &OrgFlags) -> bool {
    match t {
        Target::Resource(r) => on_resource(a) && level(p, r) >= required(a),
        Target::Branch { org, branch, .. } => in_run(p, a, *org, branch),
        Target::Org(org) => p.org() == *org && on_org(p, a, flags),
    }
}

/// Run-internal actions: only a run token of this very branch, each kind its own actions.
fn in_run(p: &Principal, a: &Action, org: Uuid, branch: &str) -> bool {
    let Principal::RunToken {
        org: o,
        branch: b,
        kind,
        ..
    } = p
    else {
        return false;
    };
    if *o != org || b != branch {
        return false;
    }
    use Action::*;
    match kind {
        RunTokenKind::Run => matches!(a, RegisterAgent | Report | Finish),
        RunTokenKind::Agent => matches!(a, CallTools | CallModel),
        RunTokenKind::Worker | RunTokenKind::Ticket => matches!(a, WorkerLink),
    }
}

/// Org-level actions: the permission matrix by role; a team-bound service account acts as a member
/// of its team; a token also needs a scope that covers the action.
fn on_org(p: &Principal, a: &Action, flags: &OrgFlags) -> bool {
    match p {
        Principal::User { role, seats, .. } => role_allows(*role, seats, a, flags),
        Principal::ServiceAccount {
            role, team: None, ..
        } => role_allows(*role, &[], a, flags),
        Principal::ServiceAccount { team: Some(t), .. } => {
            role_allows(OrgRole::Member, &[(*t, SeatRole::Member)], a, flags)
        }
        Principal::ApiToken { holder, scopes, .. } => {
            scope_allows(scope_cap(scopes), a) && on_org(holder, a, flags)
        }
        Principal::RunToken { .. } => false,
    }
}

/// `read` covers looking (the org, usage, the audit log), `run` also starting runs and making
/// worlds and connections, `admin` everything (keys, tokens, members, settings).
fn scope_allows(top: Level, a: &Action) -> bool {
    match a {
        Action::SeeOrg | Action::Usage | Action::Audit => top >= Level::View,
        Action::CreateOrgWide(k) if *k != ResourceKind::ProviderKey => top >= Level::Edit,
        _ => top >= Level::Admin,
    }
}

/// The accounts spec's org-level permission matrix.
fn role_allows(role: OrgRole, seats: &[(Uuid, SeatRole)], a: &Action, flags: &OrgFlags) -> bool {
    use OrgRole::*;
    let admin = matches!(role, Owner | Admin);
    match a {
        Action::SeeOrg => true,
        Action::CreateOrgWide(ResourceKind::ProviderKey) => admin,
        Action::CreateOrgWide(_) => admin || role == Member,
        Action::CreateTeam => admin || (role == Member && flags.members_create_teams),
        Action::ManageSeats(t) => {
            admin || (role == Member && seats.contains(&(*t, SeatRole::Maintainer)))
        }
        Action::Invite
        | Action::ServiceAccounts
        | Action::AnyTokens
        | Action::Audit
        | Action::OrgSettings => admin,
        Action::ChangeRole { target } => role == Owner || (role == Admin && *target != Owner),
        Action::OwnTokens => role != Billing,
        Action::Usage => admin || role == Billing,
        Action::Billing => matches!(role, Owner | Billing),
        Action::DeleteOrg => role == Owner,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use OrgRole::*;

    const A: Uuid = Uuid::from_u128(0xa0);
    const B: Uuid = Uuid::from_u128(0xb0);
    const T1: Uuid = Uuid::from_u128(0x71);
    const T2: Uuid = Uuid::from_u128(0x72);
    const ME: Uuid = Uuid::from_u128(0x01);
    const OTHER: Uuid = Uuid::from_u128(0x02);
    const F: OrgFlags = OrgFlags {
        members_create_teams: false,
    };

    fn user(role: OrgRole) -> Principal {
        Principal::User {
            user: ME,
            org: A,
            role,
            seats: vec![],
        }
    }
    fn user_of(org: Uuid) -> Principal {
        Principal::User {
            user: ME,
            org,
            role: Member,
            seats: vec![],
        }
    }
    fn seat(s: SeatRole) -> Principal {
        Principal::User {
            user: ME,
            org: A,
            role: Member,
            seats: vec![(T1, s)],
        }
    }
    fn api_token(scopes: &[TokenScope], holder: Principal) -> Principal {
        Principal::ApiToken {
            token: Uuid::from_u128(9),
            holder: Box::new(holder),
            scopes: scopes.to_vec(),
            team: None,
        }
    }
    fn team_bound_sa(team: Uuid) -> Principal {
        Principal::ServiceAccount {
            sa: Uuid::from_u128(8),
            org: A,
            role: Admin,
            team: Some(team),
        }
    }
    fn agent_tok(branch: &str, agent: &str) -> Principal {
        Principal::RunToken {
            org: A,
            run: "r".into(),
            branch: branch.into(),
            kind: RunTokenKind::Agent,
            agent: Some(agent.into()),
            sandbox: None,
        }
    }
    fn run_tok(branch: &str) -> Principal {
        Principal::RunToken {
            org: A,
            run: "r".into(),
            branch: branch.into(),
            kind: RunTokenKind::Run,
            agent: None,
            sandbox: None,
        }
    }
    fn resource(team: Option<Uuid>, created_by: Option<Uuid>, grants: Vec<Grant>) -> Resource {
        Resource {
            kind: ResourceKind::Run,
            org: A,
            id: "r".into(),
            team,
            created_by,
            grants,
        }
    }
    fn org_wide_run() -> Resource {
        resource(None, Some(OTHER), vec![])
    }
    fn team_run() -> Resource {
        team_run_of(T1)
    }
    fn team_run_of(t: Uuid) -> Resource {
        resource(Some(t), Some(OTHER), vec![])
    }
    fn granted(level: Level) -> Resource {
        resource(
            Some(T2),
            Some(OTHER),
            vec![Grant {
                to: Grantee::User(ME),
                level,
            }],
        )
    }
    fn branch<'a>(org: Uuid, run: &'a str, b: &'a str) -> Target<'a> {
        Target::Branch {
            org,
            run,
            branch: b,
        }
    }

    use Action as X;
    use ResourceKind as K;
    /// The org-level permission matrix of the accounts spec §3: (role, action, allowed).
    type MatrixRow = (fn() -> Action, [bool; 5]);
    const MATRIX: &[MatrixRow] = &[
        //                                                   owner  admin  member viewer billing
        (|| X::SeeOrg, [true, true, true, true, true]),
        (
            || X::CreateOrgWide(K::ProviderKey),
            [true, true, false, false, false],
        ),
        (
            || X::CreateOrgWide(K::World),
            [true, true, true, false, false],
        ),
        (
            || X::CreateOrgWide(K::Connection),
            [true, true, true, false, false],
        ),
        (
            || X::CreateOrgWide(K::Run),
            [true, true, true, false, false],
        ),
        (|| X::CreateTeam, [true, true, false, false, false]),
        (|| X::ManageSeats(T1), [true, true, false, false, false]),
        (|| X::Invite, [true, true, false, false, false]),
        (
            || X::ChangeRole { target: Member },
            [true, true, false, false, false],
        ),
        (|| X::ServiceAccounts, [true, true, false, false, false]),
        (|| X::AnyTokens, [true, true, false, false, false]),
        (|| X::OwnTokens, [true, true, true, true, false]),
        (|| X::Audit, [true, true, false, false, false]),
        (|| X::Usage, [true, true, false, false, true]),
        (|| X::Billing, [true, false, false, false, true]),
        (|| X::OrgSettings, [true, true, false, false, false]),
        (|| X::DeleteOrg, [true, false, false, false, false]),
    ];

    #[test]
    fn org_matrix() {
        let roles = [Owner, Admin, Member, Viewer, Billing];
        for (action, want) in MATRIX {
            for (role, want) in roles.iter().zip(want) {
                let a = action();
                assert_eq!(
                    can(&user(*role), &a, &Target::Org(A), &F),
                    *want,
                    "{role:?} {a:?}"
                );
                assert!(
                    !can(&user(*role), &a, &Target::Org(B), &F),
                    "{role:?} {a:?} in another org"
                );
            }
        }
    }

    #[test]
    fn admins_cannot_change_owners() {
        assert!(!can(
            &user(Admin),
            &Action::ChangeRole { target: Owner },
            &Target::Org(A),
            &F
        ));
        assert!(can(
            &user(Owner),
            &Action::ChangeRole { target: Owner },
            &Target::Org(A),
            &F
        ));
    }

    #[test]
    fn members_create_teams_only_when_the_org_allows() {
        let allows = OrgFlags {
            members_create_teams: true,
        };
        assert!(!can(
            &user(Member),
            &Action::CreateTeam,
            &Target::Org(A),
            &F
        ));
        assert!(can(
            &user(Member),
            &Action::CreateTeam,
            &Target::Org(A),
            &allows
        ));
        assert!(!can(
            &user(Viewer),
            &Action::CreateTeam,
            &Target::Org(A),
            &allows
        ));
    }

    #[test]
    fn maintainers_manage_their_own_teams_seats() {
        let m = seat(SeatRole::Maintainer);
        assert!(can(&m, &Action::ManageSeats(T1), &Target::Org(A), &F));
        assert!(!can(&m, &Action::ManageSeats(T2), &Target::Org(A), &F));
        assert!(!can(
            &seat(SeatRole::Member),
            &Action::ManageSeats(T1),
            &Target::Org(A),
            &F
        ));
    }

    fn level_cases() -> Vec<(Principal, Resource, Level)> {
        vec![
            (user(Member), org_wide_run(), Level::Run),
            (user(Member), team_run(), Level::None),
            (seat(SeatRole::Member), team_run(), Level::Edit),
            (seat(SeatRole::Maintainer), team_run(), Level::Admin),
            (
                user(Member),
                resource(Some(T1), Some(ME), vec![]),
                Level::Admin,
            ),
            (user(Admin), team_run(), Level::Admin),
            (user(Member), granted(Level::Run), Level::Run),
            (user(Viewer), granted(Level::Admin), Level::View),
            (user(Viewer), org_wide_run(), Level::View),
            (user(Billing), org_wide_run(), Level::None),
            (user_of(B), org_wide_run(), Level::None),
        ]
    }

    /// Tokens, service accounts and shares through a team.
    fn bound_level_cases() -> Vec<(Principal, Resource, Level)> {
        use TokenScope as S;
        let owner_tok = |s: TokenScope| api_token(&[s], user(Owner));
        let team_grant = resource(
            Some(T2),
            Some(OTHER),
            vec![Grant {
                to: Grantee::Team(T1),
                level: Level::View,
            }],
        );
        vec![
            (owner_tok(S::Read), org_wide_run(), Level::View),
            (owner_tok(S::Run), org_wide_run(), Level::Edit),
            (owner_tok(S::Admin), org_wide_run(), Level::Admin),
            (team_bound_sa(T2), team_run_of(T1), Level::None),
            (team_bound_sa(T1), team_run_of(T1), Level::Edit),
            (team_bound_sa(T1), org_wide_run(), Level::Run),
            (seat(SeatRole::Member), team_grant, Level::View),
            (run_tok("b1"), org_wide_run(), Level::None),
        ]
    }

    #[test]
    fn levels() {
        for (p, r, want) in level_cases().into_iter().chain(bound_level_cases()) {
            assert_eq!(level(&p, &r), want, "{p:?} on {r:?}");
        }
    }

    #[test]
    fn resource_actions_need_their_levels() {
        for (a, need) in RESOURCE_ACTIONS.iter().map(|(a, n)| (a(), *n)) {
            assert_eq!(required(&a), need, "{a:?}");
            let r = org_wide_run();
            let on = |p: &Principal| can(p, &a, &Target::Resource(&r), &F);
            assert_eq!(on(&user(Member)), need <= Level::Run, "member {a:?}");
            assert_eq!(on(&user(Viewer)), need <= Level::View, "viewer {a:?}");
        }
    }

    type ActionLevel = (fn() -> Action, Level);
    const RESOURCE_ACTIONS: &[ActionLevel] = &[
        (|| Action::Read, Level::View),
        (|| Action::Start, Level::Run),
        (|| Action::Fork, Level::Run),
        (|| Action::Stop, Level::Run),
        (|| Action::Record, Level::Run),
        (|| Action::UseInRun, Level::Run),
        (|| Action::Rename, Level::Edit),
        (|| Action::ChangeSetup, Level::Edit),
        (|| Action::ReplaceSecret, Level::Edit),
        (|| Action::DeleteFork, Level::Edit),
        (|| Action::Share, Level::Admin),
        (|| Action::MoveTeam, Level::Admin),
        (|| Action::Delete, Level::Admin),
    ];

    #[test]
    fn run_tokens_reach_only_their_branch_and_agent() {
        let (agent, run) = (agent_tok("b1", "a"), run_tok("b1"));
        let r = org_wide_run();
        let cases: Vec<(&Principal, Action, Target<'_>, bool)> = vec![
            (&agent, Action::CallTools, branch(A, "r", "b1"), true),
            (&agent, Action::CallModel, branch(A, "r", "b1"), true),
            (&agent, Action::CallTools, branch(A, "r", "b2"), false),
            (&agent, Action::CallTools, branch(B, "r", "b1"), false),
            (&agent, Action::Finish, branch(A, "r", "b1"), false),
            (&agent, Action::Read, Target::Resource(&r), false),
            (&run, Action::Finish, branch(A, "r", "b1"), true),
            (&run, Action::RegisterAgent, branch(A, "r", "b1"), true),
            (&run, Action::CallTools, branch(A, "r", "b1"), false),
            (&run, Action::SeeOrg, Target::Org(A), false),
        ];
        for (p, a, t, want) in cases {
            assert_eq!(can(p, &a, &t, &F), want, "{p:?} {a:?} {t:?}");
        }
        assert!(!can(
            &user(Owner),
            &Action::CallTools,
            &branch(A, "r", "b1"),
            &F
        ));
    }

    #[test]
    fn api_tokens_cap_org_actions_too() {
        use TokenScope as S;
        let in_a = |p: &Principal, a: Action| can(p, &a, &Target::Org(A), &F);
        let (read, run) = (
            api_token(&[S::Read], user(Owner)),
            api_token(&[S::Run], user(Owner)),
        );
        assert!(in_a(&read, Action::SeeOrg));
        assert!(!in_a(&read, Action::CreateOrgWide(K::Run)));
        assert!(!in_a(&read, Action::AnyTokens));
        assert!(in_a(&run, Action::CreateOrgWide(K::Run)));
        assert!(!in_a(&run, Action::CreateOrgWide(K::ProviderKey)));
        let admin = api_token(&[S::Admin], user(Owner));
        assert!(in_a(&admin, Action::CreateOrgWide(K::ProviderKey)));
        let member_admin = api_token(&[S::Admin], user(Member));
        assert!(!in_a(&member_admin, Action::AnyTokens));
    }
}
