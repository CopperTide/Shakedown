//! A `Scope`: who a store transaction runs as. Only the auth layer builds one, from a principal;
//! `Store::tx` turns it into the `app.*` settings every row-level security policy reads.

use uuid::Uuid;

use super::principal::{OrgRole, Principal};
use crate::store::{LOCAL_ORG_ID, LOCAL_USER_ID};

/// The org, user, role and teams a transaction acts with, and the principal they came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    org: Uuid,
    user: Option<Uuid>,
    role: OrgRole,
    teams: Vec<Uuid>,
    principal: Principal,
}

impl Scope {
    /// A team-bound principal acts as a member of its team only, with no user.
    pub(in crate::auth) fn of(p: Principal) -> Scope {
        let (user, role, teams) = acting_as(&p);
        Scope {
            org: p.org(),
            user,
            role,
            teams,
            principal: p,
        }
    }

    pub fn org(&self) -> Uuid {
        self.org
    }

    pub fn user(&self) -> Option<Uuid> {
        self.user
    }

    pub fn role(&self) -> OrgRole {
        self.role
    }

    pub fn teams(&self) -> &[Uuid] {
        &self.teams
    }

    pub fn principal(&self) -> &Principal {
        &self.principal
    }

    /// `app.org_id`, `app.user_id` (empty for none), `app.team_ids` (a comma list) and
    /// `app.org_role` (lower-case), as `Store::tx` sets them.
    pub(crate) fn settings(&self) -> [String; 4] {
        let teams: Vec<String> = self.teams.iter().map(Uuid::to_string).collect();
        [
            self.org.to_string(),
            self.user.map(|u| u.to_string()).unwrap_or_default(),
            teams.join(","),
            self.role.as_str().to_string(),
        ]
    }
}

/// The user, role and teams `p` acts with.
fn acting_as(p: &Principal) -> (Option<Uuid>, OrgRole, Vec<Uuid>) {
    match p {
        Principal::User {
            user, role, seats, ..
        } => (Some(*user), *role, seats.iter().map(|(t, _)| *t).collect()),
        Principal::ServiceAccount { team: Some(t), .. }
        | Principal::ApiToken { team: Some(t), .. } => (None, OrgRole::Member, vec![*t]),
        Principal::ServiceAccount {
            role, team: None, ..
        } => (None, *role, vec![]),
        Principal::ApiToken { holder, .. } => acting_as(holder),
        Principal::RunToken { .. } => (None, OrgRole::Member, vec![]),
    }
}

/// The built-in owner of a local shaked: every local request runs as it.
pub fn local_owner() -> Scope {
    Scope::of(Principal::User {
        user: LOCAL_USER_ID,
        org: LOCAL_ORG_ID,
        role: OrgRole::Owner,
        seats: vec![],
    })
}

/// A scope for `p`, for tests (the auth layer is the only other builder).
#[cfg(any(test, feature = "testkit"))]
pub fn test_scope(p: Principal) -> Scope {
    Scope::of(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{SeatRole, TokenScope};

    const ORG: Uuid = Uuid::from_u128(1);
    const TEAM: Uuid = Uuid::from_u128(2);
    const USER: Uuid = Uuid::from_u128(3);

    fn owner_with_seat() -> Principal {
        Principal::User {
            user: USER,
            org: ORG,
            role: OrgRole::Owner,
            seats: vec![(TEAM, SeatRole::Member)],
        }
    }

    #[test]
    fn a_user_acts_with_its_role_and_teams() {
        let want = [
            ORG.to_string(),
            USER.to_string(),
            TEAM.to_string(),
            "owner".to_string(),
        ];
        assert_eq!(test_scope(owner_with_seat()).settings(), want);
        assert_eq!(local_owner().settings()[3], "owner");
    }

    #[test]
    fn team_bound_principals_act_as_members_of_their_team() {
        let holder = Box::new(owner_with_seat());
        let bound = Principal::ApiToken {
            token: Uuid::nil(),
            holder,
            scopes: vec![TokenScope::Admin],
            team: Some(TEAM),
        };
        let want = [
            ORG.to_string(),
            String::new(),
            TEAM.to_string(),
            "member".to_string(),
        ];
        assert_eq!(test_scope(bound).settings(), want);
    }
}
