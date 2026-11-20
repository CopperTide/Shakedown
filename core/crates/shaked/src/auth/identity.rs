//! Who a signed user is in an org, read live from `identity` on every request: removing a member
//! or deleting an org takes effect on the next request.

use anyhow::Result;
use serde::Serialize;
use uuid::Uuid;

use super::{OrgRole, Principal, Scope, SeatRole};
use crate::store::Store;

/// An org as `/api/me` lists it, with the caller's role there.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrgSummary {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub kind: String,
    #[serde(serialize_with = "role_name")]
    pub role: OrgRole,
}

fn role_name<S: serde::Serializer>(r: &OrgRole, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(r.as_str())
}

const MEMBER: &str = "SELECT m.role FROM identity.memberships m
     JOIN identity.orgs o ON o.id = m.org_id
     JOIN identity.users u ON u.id = m.user_id
    WHERE m.org_id = $1 AND m.user_id = $2 AND o.deleted_at IS NULL AND u.disabled_at IS NULL";
const SEATS: &str =
    "SELECT team_id, role FROM identity.team_members WHERE org_id = $1 AND user_id = $2";

/// `user` as a member of `org`, with their role and seats; None when they are not a member, are
/// disabled, or the org is deleted. Read under the org's wall (row-level security), not as a
/// system job.
pub async fn principal_of_user(store: &Store, user: Uuid, org: Uuid) -> Result<Option<Principal>> {
    let probe = Scope::of(Principal::User {
        user,
        org,
        role: OrgRole::Viewer,
        seats: vec![],
    });
    let tx = store.tx(&probe).await?;
    let Some(row) = tx.query_opt(MEMBER, &[&org, &user]).await? else {
        return Ok(None);
    };
    let role = OrgRole::parse(row.get::<_, &str>(0)).unwrap_or(OrgRole::Viewer);
    let seats = tx.query(SEATS, &[&org, &user]).await?;
    let seats = seats
        .iter()
        .map(|r| (r.get(0), seat_role(r.get(1))))
        .collect();
    tx.commit().await?;
    Ok(Some(Principal::User {
        user,
        org,
        role,
        seats,
    }))
}

fn seat_role(s: &str) -> SeatRole {
    match s {
        "maintainer" => SeatRole::Maintainer,
        _ => SeatRole::Member,
    }
}

/// The scope's own org, as the scope itself reads it.
pub async fn org_of(store: &Store, scope: &Scope) -> Result<OrgSummary> {
    let sql = "SELECT o.id, o.slug, o.name, o.kind, $2::text AS role FROM identity.orgs o WHERE o.id = $1";
    let tx = store.tx(scope).await?;
    let row = tx
        .query_one(sql, &[&scope.org(), &scope.role().as_str()])
        .await?;
    tx.commit().await?;
    Ok(summary_of(&row))
}

fn summary_of(r: &tokio_postgres::Row) -> OrgSummary {
    OrgSummary {
        id: r.get(0),
        slug: r.get(1),
        name: r.get(2),
        kind: r.get(3),
        role: OrgRole::parse(r.get::<_, &str>(4)).unwrap_or(OrgRole::Viewer),
    }
}
