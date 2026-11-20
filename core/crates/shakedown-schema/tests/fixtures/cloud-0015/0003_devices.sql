CREATE TABLE devices (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name text NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
  platform text NOT NULL,
  key_hash text NOT NULL UNIQUE,
  created_at timestamptz NOT NULL DEFAULT now(),
  last_seen timestamptz,
  revoked_at timestamptz
);
CREATE INDEX devices_user ON devices(user_id);
CREATE TABLE device_codes (
  device_code_hash text PRIMARY KEY,
  user_code text NOT NULL UNIQUE,
  name text NOT NULL,
  platform text NOT NULL,
  approved_by uuid REFERENCES users(id) ON DELETE CASCADE,
  expires_at timestamptz NOT NULL
);
