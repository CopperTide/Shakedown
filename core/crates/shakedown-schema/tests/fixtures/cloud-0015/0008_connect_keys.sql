-- One-time keys for connecting a computer from a copied command (no code to approve). Hashed; one use; 30 minutes.
CREATE TABLE connect_keys (
  key_hash text PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL
);
CREATE INDEX connect_keys_user ON connect_keys(user_id);
