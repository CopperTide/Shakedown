CREATE TABLE users (
  id uuid PRIMARY KEY,
  name text NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
  email text NOT NULL,
  role text NOT NULL DEFAULT 'member' CHECK (role IN ('member','admin')),
  created_at timestamptz NOT NULL DEFAULT now(),
  disabled_at timestamptz
);
CREATE TABLE identities (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider text NOT NULL CHECK (provider IN ('google','github','password')),
  subject text NOT NULL,
  password_hash text,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (provider, subject)
);
CREATE TABLE invites (
  id uuid PRIMARY KEY,
  token_hash text NOT NULL UNIQUE,
  kind text NOT NULL DEFAULT 'join' CHECK (kind IN ('join','reset')),
  role text NOT NULL DEFAULT 'member' CHECK (role IN ('member','admin')),
  name text NOT NULL,
  email text,
  note text,
  for_user uuid REFERENCES users(id) ON DELETE CASCADE,
  created_by uuid REFERENCES users(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL,
  used_at timestamptz,
  used_by uuid REFERENCES users(id) ON DELETE SET NULL,
  revoked_at timestamptz
);
CREATE TABLE sessions (
  token_hash text PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  csrf text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  last_seen timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL,
  user_agent text
);
CREATE INDEX sessions_user ON sessions(user_id);
CREATE TABLE login_flows (
  state_hash text PRIMARY KEY,
  browser_hash text NOT NULL,
  provider text NOT NULL CHECK (provider IN ('google','github')),
  purpose text NOT NULL CHECK (purpose IN ('join','login')),
  invite_id uuid REFERENCES invites(id) ON DELETE CASCADE,
  nonce text,
  verifier text,
  expires_at timestamptz NOT NULL
);
CREATE TABLE limits (
  key text PRIMARY KEY,
  count bigint NOT NULL,
  expires_at timestamptz NOT NULL
);
