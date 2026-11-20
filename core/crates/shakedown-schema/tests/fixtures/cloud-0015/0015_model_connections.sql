-- Model connections: a user's Anthropic or OpenRouter key, for their agents. The key is sealed with the
-- server's vault key (not in this database), never sent back to a browser (only its last 4 characters),
-- and deleted with its connection.
CREATE TABLE model_connections (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name text NOT NULL CHECK (length(name) BETWEEN 1 AND 60),
  provider text NOT NULL CHECK (provider IN ('anthropic-key','openrouter')),
  model text CHECK (length(model) <= 100),
  sealed_key bytea NOT NULL,
  key_last4 text NOT NULL,
  -- The last Check (it spends no tokens): NULL until one ran.
  checked_ok boolean,
  checked_at timestamptz,
  check_detail text NOT NULL DEFAULT '',
  created_at timestamptz NOT NULL DEFAULT now(),
  CHECK (provider <> 'openrouter' OR model IS NOT NULL)
);
CREATE INDEX model_connections_user ON model_connections(user_id);

-- Every add, check and delete, kept after the connection is deleted. No keys.
CREATE TABLE connection_events (
  id bigserial PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  connection_id uuid NOT NULL,
  at timestamptz NOT NULL DEFAULT now(),
  action text NOT NULL,
  ok boolean NOT NULL,
  detail text NOT NULL DEFAULT ''
);
CREATE INDEX connection_events_user ON connection_events(user_id, id DESC);

-- The ops site's read-only viewer gets SELECT on new tables by default: in this same transaction it
-- loses the sealed column, so keys are never readable, not even until its setup script runs again.
DO $$
BEGIN
  IF EXISTS (SELECT FROM pg_roles WHERE rolname = 'ops_readonly') THEN
    REVOKE ALL ON model_connections FROM ops_readonly;
    GRANT SELECT (id, user_id, name, provider, model, key_last4, checked_ok, checked_at, check_detail, created_at) ON model_connections TO ops_readonly;
  END IF;
END $$;
