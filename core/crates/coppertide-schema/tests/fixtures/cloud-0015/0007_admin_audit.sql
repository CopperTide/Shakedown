-- Every admin action, for the admin console's audit log.
CREATE TABLE admin_audit (
  id bigserial PRIMARY KEY,
  at timestamptz NOT NULL DEFAULT now(),
  admin_id uuid REFERENCES users(id) ON DELETE SET NULL,
  action text NOT NULL,
  target_id uuid REFERENCES users(id) ON DELETE SET NULL,
  detail jsonb NOT NULL DEFAULT '{}'
);
CREATE INDEX admin_audit_at ON admin_audit(at DESC);
