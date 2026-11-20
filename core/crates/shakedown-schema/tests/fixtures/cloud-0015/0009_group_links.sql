-- Group links: one short, reusable join link for a class or an event (/join/cs2680), and which
-- invite or link each user joined through.
ALTER TABLE invites
  ADD COLUMN slug text UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,39}$'),
  ADD COLUMN max_uses integer CHECK (max_uses > 0),
  ADD COLUMN uses integer NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN joined_via uuid REFERENCES invites(id) ON DELETE SET NULL;
UPDATE users u SET joined_via = i.id FROM invites i WHERE i.used_by = u.id AND i.kind = 'join';
