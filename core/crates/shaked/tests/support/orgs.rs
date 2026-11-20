//! The tenancy fixtures (`tests/fixtures/orgs.toml`): two orgs, a team in each and one user per
//! role, seeded into a test database, and scopes for them.
#![allow(dead_code)]

use serde::Deserialize;
use shaked::auth::{OrgRole, Principal, SeatRole};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct Orgs {
    pub org: Vec<Org>,
    pub user: Vec<User>,
}

#[derive(Debug, Deserialize)]
pub struct Org {
    pub key: String,
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub team: Team,
}

#[derive(Debug, Deserialize)]
pub struct Team {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub key: String,
    pub id: Uuid,
    pub org: String,
    pub role: String,
    #[serde(default)]
    pub seat: Option<String>,
}

/// The fixtures, parsed.
pub fn fixtures() -> Orgs {
    let text = include_str!("../fixtures/orgs.toml");
    toml::from_str(text).expect("tests/fixtures/orgs.toml parses")
}

impl Orgs {
    pub fn org(&self, key: &str) -> &Org {
        self.org
            .iter()
            .find(|o| o.key == key)
            .unwrap_or_else(|| panic!("no org {key}"))
    }

    pub fn user(&self, key: &str) -> &User {
        self.user
            .iter()
            .find(|u| u.key == key)
            .unwrap_or_else(|| panic!("no user {key}"))
    }

    /// The principal of user `key`, with its seat.
    pub fn principal(&self, key: &str) -> Principal {
        let u = self.user(key);
        let org = self.org(&u.org);
        let seats = match u.seat.as_deref() {
            Some("maintainer") => vec![(org.team.id, SeatRole::Maintainer)],
            Some(_) => vec![(org.team.id, SeatRole::Member)],
            None => vec![],
        };
        let role = OrgRole::parse(&u.role).expect("a known role");
        Principal::User {
            user: u.id,
            org: org.id,
            role,
            seats,
        }
    }

    /// The SQL that seeds every org, team, user, membership and seat (run as a superuser).
    pub fn seed_sql(&self) -> String {
        let mut sql = String::new();
        for o in &self.org {
            sql += &format!(
                "INSERT INTO identity.orgs (id, slug, name, kind) VALUES ('{}', '{}', '{}', 'customer');
                 INSERT INTO identity.teams (id, org_id, slug, name) VALUES ('{}', '{}', '{}', '{}');\n",
                o.id, o.slug, o.name, o.team.id, o.id, o.team.slug, o.team.name
            );
        }
        for u in &self.user {
            let org = self.org(&u.org);
            sql += &format!(
                "INSERT INTO identity.users (id, name, email) VALUES ('{}', '{}', '{}@example.test');
                 INSERT INTO identity.memberships (org_id, user_id, role) VALUES ('{}', '{}', '{}');\n",
                u.id, u.key, u.key, org.id, u.id, u.role
            );
            if let Some(seat) = &u.seat {
                sql += &format!(
                    "INSERT INTO identity.team_members (org_id, team_id, user_id, role) VALUES ('{}', '{}', '{}', '{seat}');\n",
                    org.id, org.team.id, u.id
                );
            }
        }
        sql
    }
}

/// Seeds the fixtures into database `db`.
pub async fn seed(db: &str) -> Orgs {
    let orgs = fixtures();
    super::pg::execute(db, &orgs.seed_sql()).await;
    orgs
}
