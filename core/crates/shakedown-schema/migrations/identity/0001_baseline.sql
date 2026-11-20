-- identity 1: Cloud's sign-in tables as its migrations 0001–0015 leave them. A database that has them in public
-- (Cloud before R2) moves them here, which copies nothing; any other database (fresh dev, tests, a local shaked)
-- creates them. tests/migrate.rs checks that both paths give the same schema.
CREATE SCHEMA IF NOT EXISTS identity;

DO $$
DECLARE t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['users','identities','invites','sessions','login_flows','limits','admin_audit'] LOOP
    IF to_regclass('public.' || t) IS NOT NULL AND to_regclass('identity.' || t) IS NULL THEN
      EXECUTE format('ALTER TABLE public.%I SET SCHEMA identity', t);
    END IF;
  END LOOP;
END $$;

CREATE TABLE IF NOT EXISTS identity.users (
  id uuid PRIMARY KEY,
  name text NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
  email text NOT NULL,
  role text NOT NULL DEFAULT 'member' CHECK (role IN ('member','admin')),
  created_at timestamptz NOT NULL DEFAULT now(),
  disabled_at timestamptz,
  joined_via uuid,
  workspace boolean NOT NULL DEFAULT true
);
CREATE UNIQUE INDEX IF NOT EXISTS users_email_unique ON identity.users (lower(email));

CREATE TABLE IF NOT EXISTS identity.identities (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
  provider text NOT NULL CHECK (provider IN ('google','github','password')),
  subject text NOT NULL,
  password_hash text,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (provider, subject)
);

CREATE TABLE IF NOT EXISTS identity.invites (
  id uuid PRIMARY KEY,
  token_hash text NOT NULL UNIQUE,
  kind text NOT NULL DEFAULT 'join' CHECK (kind IN ('join','reset')),
  role text NOT NULL DEFAULT 'member' CHECK (role IN ('member','admin')),
  name text NOT NULL,
  email text,
  note text,
  for_user uuid REFERENCES identity.users(id) ON DELETE CASCADE,
  created_by uuid REFERENCES identity.users(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL,
  used_at timestamptz,
  used_by uuid REFERENCES identity.users(id) ON DELETE SET NULL,
  revoked_at timestamptz,
  slug text UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,39}$'),
  max_uses integer CHECK (max_uses > 0),
  uses integer NOT NULL DEFAULT 0
);

DO $$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_constraint
                  WHERE conname = 'users_joined_via_fkey' AND conrelid = 'identity.users'::regclass) THEN
    ALTER TABLE identity.users
      ADD CONSTRAINT users_joined_via_fkey FOREIGN KEY (joined_via) REFERENCES identity.invites(id) ON DELETE SET NULL;
  END IF;
END $$;

CREATE TABLE IF NOT EXISTS identity.sessions (
  token_hash text PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
  csrf text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  last_seen timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL,
  user_agent text
);
CREATE INDEX IF NOT EXISTS sessions_user ON identity.sessions (user_id);

CREATE TABLE IF NOT EXISTS identity.login_flows (
  state_hash text PRIMARY KEY,
  browser_hash text NOT NULL,
  provider text NOT NULL CHECK (provider IN ('google','github')),
  purpose text NOT NULL CONSTRAINT login_flows_purpose_check CHECK (purpose IN ('join','login','claim')),
  invite_id uuid REFERENCES identity.invites(id) ON DELETE CASCADE,
  nonce text,
  verifier text,
  expires_at timestamptz NOT NULL,
  user_id uuid REFERENCES identity.users(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS identity.limits (
  key text PRIMARY KEY,
  count bigint NOT NULL,
  expires_at timestamptz NOT NULL
);

CREATE TABLE IF NOT EXISTS identity.admin_audit (
  id bigserial PRIMARY KEY,
  at timestamptz NOT NULL DEFAULT now(),
  admin_id uuid REFERENCES identity.users(id) ON DELETE SET NULL,
  action text NOT NULL,
  target_id uuid REFERENCES identity.users(id) ON DELETE SET NULL,
  detail jsonb NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS admin_audit_at ON identity.admin_audit (at DESC);
