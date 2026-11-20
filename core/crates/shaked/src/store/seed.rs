//! The built-in owner of a local shaked: one user, the `local` org and the owner membership, with
//! fixed ids. Code, not a migration, so a hosted database never has them.

use anyhow::Result;
use uuid::Uuid;

/// The local user (`00000000-0000-0000-0000-00000000000a`).
pub const LOCAL_USER_ID: Uuid = Uuid::from_u128(0x0a);
/// The local org (`00000000-0000-0000-0000-00000000000b`), of kind `local` and plan `local`.
pub const LOCAL_ORG_ID: Uuid = Uuid::from_u128(0x0b);

/// The seed, idempotent (schema.sql's closing comment, with the quota plan `local`).
pub(crate) const SEED_SQL: &str = "
INSERT INTO identity.users (id, name, email)
  VALUES ('00000000-0000-0000-0000-00000000000a', 'Local', 'local@localhost') ON CONFLICT DO NOTHING;
INSERT INTO identity.orgs (id, slug, name, kind, plan)
  VALUES ('00000000-0000-0000-0000-00000000000b', 'local', 'Local', 'local', 'local') ON CONFLICT DO NOTHING;
INSERT INTO identity.memberships (org_id, user_id, role)
  VALUES ('00000000-0000-0000-0000-00000000000b', '00000000-0000-0000-0000-00000000000a', 'owner')
  ON CONFLICT DO NOTHING;";

/// Seeds the local owner on `c` (a connection that may write `identity`).
pub(crate) async fn seed_local(c: &tokio_postgres::Client) -> Result<()> {
    c.batch_execute(SEED_SQL).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_names_the_ids_it_exports() {
        assert!(SEED_SQL.contains(&LOCAL_USER_ID.to_string()));
        assert!(SEED_SQL.contains(&LOCAL_ORG_ID.to_string()));
    }
}
