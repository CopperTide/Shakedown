-- Least privilege: a community key works for Community requests only, never /host or a workspace.
ALTER TABLE devices ADD COLUMN scope text NOT NULL DEFAULT 'computer' CHECK (scope IN ('computer','community'));
ALTER TABLE device_codes ADD COLUMN scope text NOT NULL DEFAULT 'computer' CHECK (scope IN ('computer','community'));
-- "Don't allow" on the approval page: the next token poll answers access_denied (once).
ALTER TABLE device_codes ADD COLUMN denied_at timestamptz;
