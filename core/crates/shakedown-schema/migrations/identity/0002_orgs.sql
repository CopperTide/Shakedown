-- identity 2: organisations. The per-transaction principal functions (authz), orgs, memberships, flat teams and
-- seats, service accounts, one audit log, org invites, the org a browser session picked, and the grants and
-- row-level security of these tables (schema.sql §2, §3, §6 and §7, identity part). Needs bootstrap.sql's roles.
CREATE SCHEMA IF NOT EXISTS authz;

-- A missing setting reads as NULL / empty, which matches no row: fail closed.
CREATE FUNCTION authz.org_id() RETURNS uuid LANGUAGE sql STABLE PARALLEL SAFE AS
  $$ SELECT nullif(current_setting('app.org_id', true), '')::uuid $$;
CREATE FUNCTION authz.user_id() RETURNS uuid LANGUAGE sql STABLE PARALLEL SAFE AS
  $$ SELECT nullif(current_setting('app.user_id', true), '')::uuid $$;
CREATE FUNCTION authz.org_role() RETURNS text LANGUAGE sql STABLE PARALLEL SAFE AS
  $$ SELECT nullif(current_setting('app.org_role', true), '') $$;
CREATE FUNCTION authz.team_ids() RETURNS uuid[] LANGUAGE sql STABLE PARALLEL SAFE AS
  $$ SELECT coalesce(string_to_array(nullif(current_setting('app.team_ids', true), ''), ',')::uuid[], '{}'::uuid[]) $$;
CREATE FUNCTION authz.is_admin() RETURNS boolean LANGUAGE sql STABLE PARALLEL SAFE AS
  $$ SELECT coalesce(authz.org_role() IN ('owner','admin'), false) $$;
-- May the principal see a root resource with this team scope and creator (grants are checked beside it)?
CREATE FUNCTION authz.sees(p_team uuid, p_creator uuid) RETURNS boolean LANGUAGE sql STABLE PARALLEL SAFE AS $$
  SELECT CASE
    WHEN authz.org_role() IN ('owner','admin') THEN true
    WHEN authz.org_role() IN ('member','viewer') THEN
      p_team IS NULL OR p_team = ANY (authz.team_ids()) OR p_creator = authz.user_id()
    ELSE false
  END $$;
-- May the principal create a root resource in this team scope (NULL = org-wide)?
CREATE FUNCTION authz.places(p_team uuid) RETURNS boolean LANGUAGE sql STABLE PARALLEL SAFE AS $$
  SELECT CASE
    WHEN authz.org_role() IN ('owner','admin') THEN true
    WHEN authz.org_role() = 'member' THEN p_team IS NULL OR p_team = ANY (authz.team_ids())
    ELSE false
  END $$;
CREATE FUNCTION authz.writer() RETURNS boolean LANGUAGE sql STABLE PARALLEL SAFE AS
  $$ SELECT coalesce(authz.org_role() IN ('owner','admin','member'), false) $$;

CREATE TABLE identity.orgs (
  id                  uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  slug                text        NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,39}$'),
  name                text        NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
  -- personal: auto-created for each user, one member; customer: a company; local: the built-in org of a local shaked.
  kind                text        NOT NULL CHECK (kind IN ('personal','customer','local')),
  personal_user_id    uuid        UNIQUE REFERENCES identity.users(id) ON DELETE CASCADE,
  plan                text        NOT NULL DEFAULT 'beta',      -- names a quota preset in config, not a price
  features            text[]      NOT NULL DEFAULT '{}',        -- dark launches, e.g. '{shakedown}'
  billing_email       text,
  billing_ref         text,                                     -- a payment provider's customer id, later
  created_by          uuid        REFERENCES identity.users(id) ON DELETE SET NULL,
  created_at          timestamptz NOT NULL DEFAULT now(),
  deleted_at          timestamptz,                              -- soft delete: inaccessible from this moment
  purge_after         timestamptz,                              -- deleted_at + retention; then rows and objects go
  shakedown_purged_at timestamptz,                              -- set by shaked's purge job; the gateway then deletes the row
  CHECK ((kind = 'personal') = (personal_user_id IS NOT NULL)),
  CHECK ((deleted_at IS NULL) = (purge_after IS NULL))
);
CREATE UNIQUE INDEX orgs_slug_unique ON identity.orgs (slug);
CREATE UNIQUE INDEX orgs_one_local ON identity.orgs (kind) WHERE kind = 'local';
CREATE INDEX orgs_purge ON identity.orgs (purge_after) WHERE deleted_at IS NOT NULL;
COMMENT ON TABLE identity.orgs IS
  'A customer: owns data, quotas, usage and (later) billing. Every resource belongs to exactly one org.';

CREATE TABLE identity.memberships (
  org_id     uuid        NOT NULL REFERENCES identity.orgs(id) ON DELETE CASCADE,
  user_id    uuid        NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
  role       text        NOT NULL CHECK (role IN ('owner','admin','member','viewer','billing')),
  invited_by uuid        REFERENCES identity.users(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (org_id, user_id)
);
CREATE INDEX memberships_user ON identity.memberships (user_id);
COMMENT ON TABLE identity.memberships IS
  'A user in an org, with one org role. Removing it cascades to team seats, grants and API tokens in that org.';

CREATE TABLE identity.teams (
  id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id      uuid        NOT NULL REFERENCES identity.orgs(id) ON DELETE CASCADE,
  slug        text        NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,39}$'),
  name        text        NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
  description text        NOT NULL DEFAULT '' CHECK (length(description) <= 500),
  created_by  uuid        REFERENCES identity.users(id) ON DELETE SET NULL,
  created_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (org_id, slug),
  UNIQUE (org_id, id)
);
COMMENT ON TABLE identity.teams IS
  'A flat group of an org''s members (people, not agent teams). Resources may be scoped to one team. A team that still scopes resources cannot be deleted (RESTRICT): they are moved first.';

CREATE TABLE identity.team_members (
  org_id     uuid        NOT NULL,
  team_id    uuid        NOT NULL,
  user_id    uuid        NOT NULL,
  role       text        NOT NULL DEFAULT 'member' CHECK (role IN ('maintainer','member')),
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (team_id, user_id),
  FOREIGN KEY (org_id, team_id) REFERENCES identity.teams(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, user_id) REFERENCES identity.memberships(org_id, user_id) ON DELETE CASCADE
);
CREATE INDEX team_members_user ON identity.team_members (org_id, user_id);
COMMENT ON TABLE identity.team_members IS 'A member''s seat on a team. Only members of the team''s org can sit on it (composite FK).';

CREATE TABLE identity.service_accounts (
  id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id      uuid        NOT NULL REFERENCES identity.orgs(id) ON DELETE CASCADE,
  team_id     uuid,
  name        text        NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
  role        text        NOT NULL DEFAULT 'member' CHECK (role IN ('admin','member','viewer')),
  created_by  uuid        REFERENCES identity.users(id) ON DELETE SET NULL,
  created_at  timestamptz NOT NULL DEFAULT now(),
  disabled_at timestamptz,
  UNIQUE (org_id, name),
  UNIQUE (org_id, id),
  FOREIGN KEY (org_id, team_id) REFERENCES identity.teams(org_id, id) ON DELETE CASCADE
);
COMMENT ON TABLE identity.service_accounts IS
  'A non-person principal of an org (CI, a customer''s scheduler). Holds API tokens; never signs in. A team-scoped one acts as a member of that team only.';

-- Org invites: join an org, creating the account if needed. Widening the kind check re-adds it under its name.
ALTER TABLE identity.invites DROP CONSTRAINT invites_kind_check;
ALTER TABLE identity.invites ADD CONSTRAINT invites_kind_check CHECK (kind IN ('join','reset','org'));
ALTER TABLE identity.invites
  ADD COLUMN org_id   uuid REFERENCES identity.orgs(id) ON DELETE CASCADE,
  ADD COLUMN org_role text CHECK (org_role IN ('owner','admin','member','viewer','billing')),
  ADD COLUMN team_id  uuid,
  ADD CONSTRAINT invites_team_fkey FOREIGN KEY (org_id, team_id)
    REFERENCES identity.teams(org_id, id) ON DELETE SET NULL (team_id),
  ADD CONSTRAINT invites_org_kind_check CHECK ((kind = 'org') = (org_id IS NOT NULL)),
  ADD CONSTRAINT invites_org_fields_check CHECK (kind <> 'org' OR (org_role IS NOT NULL AND email IS NOT NULL AND slug IS NULL)),
  ADD CONSTRAINT invites_reset_check CHECK (kind <> 'reset' OR for_user IS NOT NULL);
CREATE UNIQUE INDEX invites_one_open_per_org_email ON identity.invites (org_id, lower(email))
  WHERE kind = 'org' AND used_at IS NULL AND revoked_at IS NULL;
CREATE INDEX invites_org ON identity.invites (org_id, created_at DESC) WHERE org_id IS NOT NULL;

-- The org picked in this browser; NULL = personal org. Leaving an org clears it here.
ALTER TABLE identity.sessions
  ADD COLUMN active_org_id uuid,
  ADD CONSTRAINT sessions_active_org_fkey FOREIGN KEY (active_org_id, user_id)
    REFERENCES identity.memberships(org_id, user_id) ON DELETE SET NULL (active_org_id);

CREATE TABLE identity.audit_log (
  id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  at          timestamptz NOT NULL DEFAULT now(),
  org_id      uuid,                                       -- no FK: survives the org's purge. NULL = platform (staff) action
  team_id     uuid,
  actor_kind  text        NOT NULL CHECK (actor_kind IN ('user','service_account','api_token','run_token','staff','system')),
  actor_id    uuid,
  actor_label text        NOT NULL,                       -- email or name at the time, kept after deletion
  action      text        NOT NULL CHECK (action ~ '^[a-z_]+(\.[a-z_]+)+$'),   -- e.g. member.role_changed
  target_kind text,
  target_id   text,
  ip          inet,
  detail      jsonb       NOT NULL DEFAULT '{}'           -- never secrets or tokens
);
CREATE INDEX audit_log_org ON identity.audit_log (org_id, id DESC);
CREATE INDEX audit_log_at ON identity.audit_log (at);
COMMENT ON TABLE identity.audit_log IS
  'Append-only: who did what in which org. Written by the gateway and shaked; no role may UPDATE or DELETE except the retention job. Replaces identity.admin_audit (rows copied with org_id NULL, actor_kind staff).';

-- Privileges (schema.sql §6, identity part).
GRANT USAGE ON SCHEMA authz, identity TO gateway_app, shaked_app, shaked_system;
GRANT USAGE ON SCHEMA schema_meta TO gateway_app, shaked_app, shaked_system;
GRANT SELECT ON schema_meta.migrations TO gateway_app, shaked_app, shaked_system;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA authz TO gateway_app, shaked_app, shaked_system;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA identity TO gateway_app;
REVOKE UPDATE, DELETE ON identity.audit_log FROM gateway_app;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA identity TO gateway_app;
-- shaked reads who people are; it writes audit rows only.
GRANT SELECT ON identity.orgs, identity.memberships, identity.teams, identity.team_members, identity.service_accounts
  TO shaked_app, shaked_system;
GRANT SELECT (id, name, email, disabled_at) ON identity.users TO shaked_app, shaked_system;
GRANT SELECT, INSERT ON identity.audit_log TO shaked_app, shaked_system;
GRANT UPDATE (shakedown_purged_at) ON identity.orgs TO shaked_system;
-- The ops site's read-only viewer: never the sealed or hashed columns.
DO $$
BEGIN
  IF EXISTS (SELECT FROM pg_roles WHERE rolname = 'ops_readonly') THEN
    GRANT USAGE ON SCHEMA identity TO ops_readonly;
    GRANT SELECT ON ALL TABLES IN SCHEMA identity TO ops_readonly;
    REVOKE ALL ON identity.identities FROM ops_readonly;
    GRANT SELECT (id, user_id, provider, subject, created_at) ON identity.identities TO ops_readonly;
  END IF;
END $$;

-- Row-level security (schema.sql §7, identity part): the gateway, the identity authority, sees every row;
-- shaked sees its own org only.
ALTER TABLE identity.orgs             ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.memberships      ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.teams            ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.team_members     ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.service_accounts ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.users            ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.audit_log        ENABLE ROW LEVEL SECURITY;

CREATE POLICY gateway_all ON identity.orgs             FOR ALL TO gateway_app USING (true) WITH CHECK (true);
CREATE POLICY gateway_all ON identity.memberships      FOR ALL TO gateway_app USING (true) WITH CHECK (true);
CREATE POLICY gateway_all ON identity.teams            FOR ALL TO gateway_app USING (true) WITH CHECK (true);
CREATE POLICY gateway_all ON identity.team_members     FOR ALL TO gateway_app USING (true) WITH CHECK (true);
CREATE POLICY gateway_all ON identity.service_accounts FOR ALL TO gateway_app USING (true) WITH CHECK (true);
CREATE POLICY gateway_all ON identity.users            FOR ALL TO gateway_app USING (true) WITH CHECK (true);
CREATE POLICY gateway_all ON identity.audit_log        FOR ALL TO gateway_app USING (true) WITH CHECK (true);
-- The ops viewer (if it exists) reads every row of these; its column grants still hide secrets.
CREATE POLICY ops_read ON identity.orgs             FOR SELECT TO PUBLIC USING (current_user = 'ops_readonly');
CREATE POLICY ops_read ON identity.memberships      FOR SELECT TO PUBLIC USING (current_user = 'ops_readonly');
CREATE POLICY ops_read ON identity.teams            FOR SELECT TO PUBLIC USING (current_user = 'ops_readonly');
CREATE POLICY ops_read ON identity.team_members     FOR SELECT TO PUBLIC USING (current_user = 'ops_readonly');
CREATE POLICY ops_read ON identity.service_accounts FOR SELECT TO PUBLIC USING (current_user = 'ops_readonly');
CREATE POLICY ops_read ON identity.users            FOR SELECT TO PUBLIC USING (current_user = 'ops_readonly');
CREATE POLICY ops_read ON identity.audit_log        FOR SELECT TO PUBLIC USING (current_user = 'ops_readonly');

CREATE POLICY own_org ON identity.orgs FOR SELECT TO shaked_app USING (id = authz.org_id() AND deleted_at IS NULL);
CREATE POLICY own_org ON identity.memberships      FOR SELECT TO shaked_app USING (org_id = authz.org_id());
CREATE POLICY own_org ON identity.teams            FOR SELECT TO shaked_app USING (org_id = authz.org_id());
CREATE POLICY own_org ON identity.team_members     FOR SELECT TO shaked_app USING (org_id = authz.org_id());
CREATE POLICY own_org ON identity.service_accounts FOR SELECT TO shaked_app USING (org_id = authz.org_id());
CREATE POLICY own_org ON identity.users FOR SELECT TO shaked_app
  USING (EXISTS (SELECT 1 FROM identity.memberships m WHERE m.user_id = users.id AND m.org_id = authz.org_id()));
CREATE POLICY own_org_write ON identity.audit_log FOR INSERT TO shaked_app WITH CHECK (org_id = authz.org_id());
CREATE POLICY own_org_read ON identity.audit_log FOR SELECT TO shaked_app USING (org_id = authz.org_id() AND authz.is_admin());
