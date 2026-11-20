-- Clusters the gateway reaches over SSH to set up and keep alive a user's runner on a login node.
-- Private keys and passwords are sealed with the server's vault key (not in this database) and are
-- never sent back to a browser; removing the cluster deletes them.
CREATE TABLE clusters (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name text NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
  host text NOT NULL,
  port integer NOT NULL CHECK (port BETWEEN 1 AND 65535),
  username text NOT NULL,
  jump_host text,
  jump_port integer CHECK (jump_port BETWEEN 1 AND 65535),
  jump_username text,
  -- generated: a key pair the server made (the user adds its public half); key: a pasted private key;
  -- password: a pasted password.
  auth text NOT NULL CHECK (auth IN ('generated','key','password')),
  public_key text,
  sealed_key bytea,
  sealed_password bytea,
  -- The login node's (and jump host's) SSH host key, saved on first connect and required afterwards.
  host_key text,
  jump_host_key text,
  -- The computer (device) the runner there connects as; its key lives only on the cluster.
  device_id uuid REFERENCES devices(id) ON DELETE SET NULL,
  state text NOT NULL DEFAULT 'new' CHECK (state IN ('new','connecting','needs_answer','running','needs_code','failed')),
  detail text NOT NULL DEFAULT '',
  -- The last sign-in asked a keyboard-interactive question (MFA): reconnecting needs the user.
  needs_mfa boolean NOT NULL DEFAULT false,
  keeper text CHECK (keeper IN ('tmux','nohup')),
  installed_sha text,
  doctor text,
  doctor_at timestamptz,
  started_at timestamptz,
  last_attempt_at timestamptz,
  failures integer NOT NULL DEFAULT 0,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (user_id, host, port, username)
);
CREATE INDEX clusters_user ON clusters(user_id);

-- Every SSH action on a cluster, shown on its card. No secrets, answers or keys.
CREATE TABLE cluster_events (
  id bigserial PRIMARY KEY,
  cluster_id uuid NOT NULL REFERENCES clusters(id) ON DELETE CASCADE,
  at timestamptz NOT NULL DEFAULT now(),
  action text NOT NULL,
  ok boolean NOT NULL,
  detail text NOT NULL DEFAULT ''
);
CREATE INDEX cluster_events_cluster ON cluster_events(cluster_id, id DESC);

-- The ops site's read-only viewer gets SELECT on new tables by default: in this same transaction it
-- loses the sealed columns, so they are never readable, not even until its setup script runs again.
DO $$
BEGIN
  IF EXISTS (SELECT FROM pg_roles WHERE rolname = 'ops_readonly') THEN
    REVOKE ALL ON clusters FROM ops_readonly;
    GRANT SELECT (id, user_id, name, host, port, username, jump_host, jump_port, jump_username, auth, public_key, host_key, jump_host_key, device_id, state, detail, needs_mfa, keeper, installed_sha, doctor, doctor_at, started_at, last_attempt_at, failures, created_at) ON clusters TO ops_readonly;
  END IF;
END $$;
