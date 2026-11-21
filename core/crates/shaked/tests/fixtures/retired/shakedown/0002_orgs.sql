-- requires: identity >= 2
-- shakedown 2: every record belongs to an org (schema.sql §5–§7, shakedown part). Existing tables gain org_id
-- (defaulting to the local org until shakedown 3, so code that does not pass it yet keeps working), team_id and
-- created_by on root tables, the (org_id, id) uniques, and the schema's foreign keys as NOT VALID (validated by
-- shakedown 3). reviews keeps PRIMARY KEY (key) and gains UNIQUE (org_id, key): its keys already embed globally
-- unique ids, and swapping a primary key is a contract change. saves and worlds also name the object store their
-- object is in (store), so R2 and S3 can be switched by name. The meter checks know trial_usd_micros.

-- 1. Existing tables: the new columns and uniques.
ALTER TABLE shakedown.runs
  ADD COLUMN org_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-00000000000b',
  ADD COLUMN team_id uuid,
  ADD COLUMN created_by uuid,
  ADD COLUMN world_id uuid,
  ADD COLUMN placement text NOT NULL DEFAULT 'local' CHECK (placement IN ('local','cloud','record')),
  ADD CONSTRAINT runs_org_id_id_key UNIQUE (org_id, id);
ALTER TABLE shakedown.branches
  ADD COLUMN org_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-00000000000b',
  ADD COLUMN created_by uuid,
  ADD CONSTRAINT branches_org_id_id_key UNIQUE (org_id, id);
ALTER TABLE shakedown.events
  ADD COLUMN org_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-00000000000b';
ALTER TABLE shakedown.experiments
  ADD COLUMN org_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-00000000000b',
  ADD COLUMN team_id uuid,
  ADD COLUMN created_by uuid,
  ADD CONSTRAINT experiments_org_id_id_key UNIQUE (org_id, id);
ALTER TABLE shakedown.suites
  ADD COLUMN org_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-00000000000b',
  ADD COLUMN team_id uuid,
  ADD COLUMN created_by uuid,
  ADD CONSTRAINT suites_org_id_id_key UNIQUE (org_id, id);
ALTER TABLE shakedown.suite_runs
  ADD COLUMN org_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-00000000000b';
ALTER TABLE shakedown.reviews
  ADD COLUMN org_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-00000000000b',
  ADD COLUMN created_by uuid,
  ADD CONSTRAINT reviews_org_id_key_key UNIQUE (org_id, key);
ALTER TABLE shakedown.connections
  ADD COLUMN org_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-00000000000b',
  ADD COLUMN team_id uuid,
  ADD COLUMN created_by uuid,
  ADD COLUMN sealed_env bytea,
  ADD CONSTRAINT connections_org_id_id_key UNIQUE (org_id, id);

-- 2. New root tables the existing ones point at.
CREATE TABLE shakedown.provider_keys (
  id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id       uuid        NOT NULL REFERENCES identity.orgs(id) ON DELETE CASCADE,
  team_id      uuid,
  name         text        NOT NULL CHECK (length(name) BETWEEN 1 AND 60),
  kind         text        NOT NULL CHECK (kind IN ('anthropic','openai','openrouter','bedrock','custom')),
  base_url     text        CHECK (length(base_url) <= 500),
  model        text        CHECK (length(model) <= 100),
  sealed_key   bytea       NOT NULL,                      -- sealed to "provider_key:<org_id>:<name>"; never returned
  key_last4    text        NOT NULL,
  prices       jsonb       NOT NULL DEFAULT '{}',         -- per-model USD per token, for custom servers
  checked_ok   boolean,
  checked_at   timestamptz,
  check_detail text        NOT NULL DEFAULT '',
  created_by   uuid        REFERENCES identity.users(id) ON DELETE SET NULL,
  created_at   timestamptz NOT NULL DEFAULT now(),
  UNIQUE (org_id, name),
  UNIQUE (org_id, id),
  FOREIGN KEY (org_id, team_id) REFERENCES identity.teams(org_id, id) ON DELETE RESTRICT,
  CHECK (kind <> 'custom' OR base_url IS NOT NULL)
);
COMMENT ON TABLE shakedown.provider_keys IS
  'An org''s model key or OpenAI-compatible server. Write-only: used by the control box''s gateway, never sent to a browser or a sandbox. "run" level on it lets a person use it in runs.';

CREATE TABLE shakedown.worlds (
  id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id       uuid        NOT NULL REFERENCES identity.orgs(id) ON DELETE CASCADE,
  team_id      uuid,
  name         text        NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
  package      text        NOT NULL,
  version      text        NOT NULL,
  image_digest text,
  state        text        NOT NULL DEFAULT 'building' CHECK (state IN ('building','ready','failed','deleted')),
  store        text        NOT NULL,                      -- the object store (by name) object_key is in
  object_key   text,                                      -- <org_id>/worlds/<id>/... in object storage
  bytes        bigint      CHECK (bytes >= 0),
  error        text,
  created_by   uuid        REFERENCES identity.users(id) ON DELETE SET NULL,
  created_at   timestamptz NOT NULL DEFAULT now(),
  built_at     timestamptz,
  UNIQUE (org_id, name),
  UNIQUE (org_id, id),
  FOREIGN KEY (org_id, team_id) REFERENCES identity.teams(org_id, id) ON DELETE RESTRICT,
  CHECK (object_key IS NULL OR object_key LIKE org_id::text || '/%')
);
COMMENT ON TABLE shakedown.worlds IS
  'A world built for one org. Packages shipped in the image are not rows: every org may read them.';

CREATE TABLE shakedown.org_settings (
  org_id               uuid        PRIMARY KEY REFERENCES identity.orgs(id) ON DELETE CASCADE,
  current_world_id     uuid,
  retention_days       integer     NOT NULL DEFAULT 30 CHECK (retention_days BETWEEN 1 AND 3650),
  members_create_teams boolean     NOT NULL DEFAULT false,
  settings             jsonb       NOT NULL DEFAULT '{}',
  updated_at           timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (org_id, current_world_id) REFERENCES shakedown.worlds(org_id, id) ON DELETE SET NULL (current_world_id)
);
COMMENT ON TABLE shakedown.org_settings IS
  'Per-org Shakedown settings: the current world, retention (default 30 days, owner decision D19).';

-- 3. The existing tables' foreign keys, NOT VALID: checked for new rows now, for old ones by shakedown 3.
ALTER TABLE shakedown.runs
  ADD CONSTRAINT runs_org_fkey FOREIGN KEY (org_id) REFERENCES identity.orgs(id) ON DELETE CASCADE NOT VALID,
  ADD CONSTRAINT runs_team_fkey FOREIGN KEY (org_id, team_id)
    REFERENCES identity.teams(org_id, id) ON DELETE RESTRICT NOT VALID,
  ADD CONSTRAINT runs_created_by_fkey FOREIGN KEY (created_by)
    REFERENCES identity.users(id) ON DELETE SET NULL NOT VALID,
  ADD CONSTRAINT runs_world_fkey FOREIGN KEY (org_id, world_id)
    REFERENCES shakedown.worlds(org_id, id) ON DELETE SET NULL (world_id) NOT VALID;
ALTER TABLE shakedown.branches
  ADD CONSTRAINT branches_run_fkey FOREIGN KEY (org_id, run_id)
    REFERENCES shakedown.runs(org_id, id) ON DELETE CASCADE NOT VALID,
  ADD CONSTRAINT branches_parent_fkey FOREIGN KEY (org_id, parent_id)
    REFERENCES shakedown.branches(org_id, id) ON DELETE CASCADE NOT VALID,
  ADD CONSTRAINT branches_created_by_fkey FOREIGN KEY (created_by)
    REFERENCES identity.users(id) ON DELETE SET NULL NOT VALID;
ALTER TABLE shakedown.events
  ADD CONSTRAINT events_branch_fkey FOREIGN KEY (org_id, branch_id)
    REFERENCES shakedown.branches(org_id, id) ON DELETE CASCADE NOT VALID;
ALTER TABLE shakedown.experiments
  ADD CONSTRAINT experiments_org_fkey FOREIGN KEY (org_id) REFERENCES identity.orgs(id) ON DELETE CASCADE NOT VALID,
  ADD CONSTRAINT experiments_team_fkey FOREIGN KEY (org_id, team_id)
    REFERENCES identity.teams(org_id, id) ON DELETE RESTRICT NOT VALID,
  ADD CONSTRAINT experiments_created_by_fkey FOREIGN KEY (created_by)
    REFERENCES identity.users(id) ON DELETE SET NULL NOT VALID;
ALTER TABLE shakedown.suites
  ADD CONSTRAINT suites_org_fkey FOREIGN KEY (org_id) REFERENCES identity.orgs(id) ON DELETE CASCADE NOT VALID,
  ADD CONSTRAINT suites_team_fkey FOREIGN KEY (org_id, team_id)
    REFERENCES identity.teams(org_id, id) ON DELETE RESTRICT NOT VALID,
  ADD CONSTRAINT suites_created_by_fkey FOREIGN KEY (created_by)
    REFERENCES identity.users(id) ON DELETE SET NULL NOT VALID,
  ADD CONSTRAINT suites_experiment_fkey FOREIGN KEY (org_id, experiment_id)
    REFERENCES shakedown.experiments(org_id, id) ON DELETE CASCADE NOT VALID;
ALTER TABLE shakedown.suite_runs
  ADD CONSTRAINT suite_runs_suite_fkey FOREIGN KEY (org_id, suite_id)
    REFERENCES shakedown.suites(org_id, id) ON DELETE CASCADE NOT VALID,
  ADD CONSTRAINT suite_runs_run_fkey FOREIGN KEY (org_id, run_id)
    REFERENCES shakedown.runs(org_id, id) ON DELETE SET NULL (run_id) NOT VALID,
  ADD CONSTRAINT suite_runs_branch_fkey FOREIGN KEY (org_id, branch_id)
    REFERENCES shakedown.branches(org_id, id) ON DELETE SET NULL (branch_id) NOT VALID;
ALTER TABLE shakedown.reviews
  ADD CONSTRAINT reviews_org_fkey FOREIGN KEY (org_id) REFERENCES identity.orgs(id) ON DELETE CASCADE NOT VALID,
  ADD CONSTRAINT reviews_created_by_fkey FOREIGN KEY (created_by)
    REFERENCES identity.users(id) ON DELETE SET NULL NOT VALID;
ALTER TABLE shakedown.connections
  ADD CONSTRAINT connections_org_fkey FOREIGN KEY (org_id) REFERENCES identity.orgs(id) ON DELETE CASCADE NOT VALID,
  ADD CONSTRAINT connections_team_fkey FOREIGN KEY (org_id, team_id)
    REFERENCES identity.teams(org_id, id) ON DELETE RESTRICT NOT VALID,
  ADD CONSTRAINT connections_created_by_fkey FOREIGN KEY (created_by)
    REFERENCES identity.users(id) ON DELETE SET NULL NOT VALID;

CREATE INDEX runs_org_created ON shakedown.runs (org_id, created_at DESC);
CREATE INDEX runs_team ON shakedown.runs (org_id, team_id) WHERE team_id IS NOT NULL;
CREATE INDEX branches_run ON shakedown.branches (run_id);
CREATE INDEX branches_running ON shakedown.branches (org_id) WHERE status = 'running';
CREATE INDEX suites_experiment ON shakedown.suites (org_id, experiment_id) WHERE experiment_id IS NOT NULL;

-- 4. The other new tables.
CREATE TABLE shakedown.saves (
  id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id      uuid        NOT NULL,
  run_id      text        NOT NULL,
  branch_id   text        NOT NULL,
  integration text        NOT NULL,
  save_id     text        NOT NULL,
  seq         bigint      NOT NULL,
  store       text        NOT NULL,                       -- the object store (by name) object_key is in
  object_key  text        NOT NULL,                       -- <org_id>/saves/<integration>/<save_id>/
  bytes       bigint      CHECK (bytes >= 0),
  sha256      text,
  state       text        NOT NULL DEFAULT 'uploading' CHECK (state IN ('uploading','ready','deleted')),
  created_at  timestamptz NOT NULL DEFAULT now(),
  ready_at    timestamptz,
  expires_at  timestamptz,                                -- retention: 30 days after the run's last fork (D19)
  UNIQUE (org_id, integration, save_id),
  FOREIGN KEY (org_id, branch_id) REFERENCES shakedown.branches(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, run_id) REFERENCES shakedown.runs(org_id, id) ON DELETE CASCADE,
  CHECK (object_key LIKE org_id::text || '/%')
);
CREATE INDEX saves_branch_seq ON shakedown.saves (branch_id, seq);
CREATE INDEX saves_expiry ON shakedown.saves (expires_at) WHERE state <> 'deleted';
COMMENT ON TABLE shakedown.saves IS 'A world save uploaded to object storage, so a fork can start in a fresh sandbox. Fork-ready when state=ready.';

CREATE TABLE shakedown.sandboxes (
  id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id          uuid        NOT NULL,
  run_id          text        NOT NULL,
  branch_id       text        NOT NULL,
  backend         text        NOT NULL CHECK (backend IN ('fake','e2b','fly')),
  provider_ref    text,
  image_digest    text        NOT NULL,
  state           text        NOT NULL DEFAULT 'creating'
                    CHECK (state IN ('creating','booting','ready','stopping','deleted','failed')),
  detail          text        NOT NULL DEFAULT '',
  created_at      timestamptz NOT NULL DEFAULT now(),
  ready_at        timestamptz,
  ended_at        timestamptz,
  last_seen_at    timestamptz,
  seconds_running bigint      NOT NULL DEFAULT 0 CHECK (seconds_running >= 0),
  UNIQUE (org_id, id),
  FOREIGN KEY (org_id, branch_id) REFERENCES shakedown.branches(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, run_id) REFERENCES shakedown.runs(org_id, id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX sandboxes_one_live_per_branch ON shakedown.sandboxes (branch_id) WHERE state NOT IN ('deleted','failed');
CREATE INDEX sandboxes_live ON shakedown.sandboxes (org_id) WHERE state NOT IN ('deleted','failed');
COMMENT ON TABLE shakedown.sandboxes IS 'One cloud sandbox per branch; the reconciler''s source of truth. Live rows count against the concurrency quota.';

CREATE TABLE shakedown.api_tokens (
  id                 uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id             uuid        NOT NULL REFERENCES identity.orgs(id) ON DELETE CASCADE,
  user_id            uuid,
  service_account_id uuid,
  team_id            uuid,                                -- set: the token acts in this team only
  name               text        NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
  token_hash         text        NOT NULL UNIQUE,         -- sha256 of "sd_<64 hex>"
  token_prefix       text        NOT NULL,                -- first 8 characters, shown in lists
  scopes             text[]      NOT NULL CHECK (cardinality(scopes) > 0 AND scopes <@ ARRAY['read','run','admin']),
  created_by         uuid        REFERENCES identity.users(id) ON DELETE SET NULL,
  created_at         timestamptz NOT NULL DEFAULT now(),
  last_used_at       timestamptz,
  expires_at         timestamptz,
  revoked_at         timestamptz,
  CHECK (num_nonnulls(user_id, service_account_id) = 1),
  FOREIGN KEY (org_id, user_id) REFERENCES identity.memberships(org_id, user_id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, service_account_id) REFERENCES identity.service_accounts(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, team_id) REFERENCES identity.teams(org_id, id) ON DELETE CASCADE
);
CREATE INDEX api_tokens_org ON shakedown.api_tokens (org_id, created_at DESC);
COMMENT ON TABLE shakedown.api_tokens IS
  'SDK/CLI tokens, shown once. Act as their user or service account in one org, capped by scopes; a member leaving the org deletes theirs.';

CREATE TABLE shakedown.run_tokens (
  id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id       uuid        NOT NULL,
  run_id       text        NOT NULL,
  branch_id    text        NOT NULL,
  kind         text        NOT NULL CHECK (kind IN ('run','agent','worker','ticket')),
  agent        text,
  sandbox_id   uuid,
  token_hash   text        NOT NULL UNIQUE,
  created_at   timestamptz NOT NULL DEFAULT now(),
  expires_at   timestamptz NOT NULL,
  used_at      timestamptz,
  last_used_at timestamptz,
  revoked_at   timestamptz,
  FOREIGN KEY (org_id, branch_id) REFERENCES shakedown.branches(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, run_id) REFERENCES shakedown.runs(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, sandbox_id) REFERENCES shakedown.sandboxes(org_id, id) ON DELETE CASCADE,
  CHECK ((kind = 'agent') = (agent IS NOT NULL)),
  CHECK (kind NOT IN ('worker','ticket') OR sandbox_id IS NOT NULL)
);
CREATE INDEX run_tokens_branch ON shakedown.run_tokens (branch_id) WHERE revoked_at IS NULL;
COMMENT ON TABLE shakedown.run_tokens IS 'Short-lived machine tokens bound to one branch (and agent or sandbox). Never in events, logs or exports.';

CREATE TABLE shakedown.grants (
  id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id          uuid        NOT NULL REFERENCES identity.orgs(id) ON DELETE CASCADE,
  grantee_user_id uuid,
  grantee_team_id uuid,
  level           text        NOT NULL CHECK (level IN ('view','run','edit','admin')),
  run_id          text,
  experiment_id   text,
  suite_id        text,
  world_id        uuid,
  provider_key_id uuid,
  connection_id   text,
  resource_type   text        GENERATED ALWAYS AS (CASE
                    WHEN run_id IS NOT NULL THEN 'run'
                    WHEN experiment_id IS NOT NULL THEN 'experiment'
                    WHEN suite_id IS NOT NULL THEN 'suite'
                    WHEN world_id IS NOT NULL THEN 'world'
                    WHEN provider_key_id IS NOT NULL THEN 'provider_key'
                    ELSE 'connection' END) STORED,
  created_by      uuid        REFERENCES identity.users(id) ON DELETE SET NULL,
  created_at      timestamptz NOT NULL DEFAULT now(),
  CHECK (num_nonnulls(grantee_user_id, grantee_team_id) = 1),
  CHECK (num_nonnulls(run_id, experiment_id, suite_id, world_id, provider_key_id, connection_id) = 1),
  UNIQUE NULLS NOT DISTINCT (run_id, experiment_id, suite_id, world_id, provider_key_id, connection_id,
                             grantee_user_id, grantee_team_id),
  FOREIGN KEY (org_id, grantee_user_id) REFERENCES identity.memberships(org_id, user_id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, grantee_team_id) REFERENCES identity.teams(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, run_id) REFERENCES shakedown.runs(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, experiment_id) REFERENCES shakedown.experiments(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, suite_id) REFERENCES shakedown.suites(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, world_id) REFERENCES shakedown.worlds(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, provider_key_id) REFERENCES shakedown.provider_keys(org_id, id) ON DELETE CASCADE,
  FOREIGN KEY (org_id, connection_id) REFERENCES shakedown.connections(org_id, id) ON DELETE CASCADE
);
CREATE INDEX grants_run ON shakedown.grants (run_id) WHERE run_id IS NOT NULL;
CREATE INDEX grants_experiment ON shakedown.grants (experiment_id) WHERE experiment_id IS NOT NULL;
CREATE INDEX grants_suite ON shakedown.grants (suite_id) WHERE suite_id IS NOT NULL;
CREATE INDEX grants_world ON shakedown.grants (world_id) WHERE world_id IS NOT NULL;
CREATE INDEX grants_provider_key ON shakedown.grants (provider_key_id) WHERE provider_key_id IS NOT NULL;
CREATE INDEX grants_connection ON shakedown.grants (connection_id) WHERE connection_id IS NOT NULL;
CREATE INDEX grants_user ON shakedown.grants (org_id, grantee_user_id) WHERE grantee_user_id IS NOT NULL;
CREATE INDEX grants_team ON shakedown.grants (org_id, grantee_team_id) WHERE grantee_team_id IS NOT NULL;
COMMENT ON TABLE shakedown.grants IS
  'Shares one resource with a member or a team of the same org at a level. Cross-org sharing is not possible by construction.';

CREATE TABLE shakedown.quotas (
  org_id      uuid        NOT NULL REFERENCES identity.orgs(id) ON DELETE CASCADE,
  meter       text        NOT NULL CHECK (meter IN ('concurrent_sandboxes','sandbox_seconds','branch_seconds_max',
                                                    'saves_bytes','model_usd_micros','trial_usd_micros','runs','seats')),
  period      text        NOT NULL CHECK (period IN ('month','none')),   -- none: a gauge or a per-item cap
  limit_value bigint      NOT NULL CHECK (limit_value >= 0),
  hard        boolean     NOT NULL DEFAULT true,                         -- false: warn only
  set_by      uuid        REFERENCES identity.users(id) ON DELETE SET NULL,
  updated_at  timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (org_id, meter)
);
COMMENT ON TABLE shakedown.quotas IS
  'Per-org overrides; absent rows fall back to the org plan''s preset in config. Set by staff only.';

CREATE TABLE shakedown.usage (
  id              bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  org_id          uuid        NOT NULL REFERENCES identity.orgs(id) ON DELETE CASCADE,
  team_id         uuid,                                   -- no FK: usage keeps its team after the team goes
  user_id         uuid,                                   -- who started the run, if a person
  run_id          text,
  branch_id       text,
  sandbox_id      uuid,
  meter           text        NOT NULL CHECK (meter IN ('sandbox_seconds','model_usd_micros','model_tokens',
                                                          'saves_bytes','runs','trial_usd_micros')),
  quantity        bigint      NOT NULL,
  at              timestamptz NOT NULL DEFAULT now(),
  source          text        NOT NULL CHECK (source IN ('sandbox','gateway','storage','api','system')),
  idempotency_key text        NOT NULL UNIQUE             -- e.g. gateway:<branch>:<seq>; resends count once
);
CREATE INDEX usage_org_meter_at ON shakedown.usage (org_id, meter, at);
COMMENT ON TABLE shakedown.usage IS 'Append-only metering ledger per org (no prices, no payments). Kept 13 months, then only usage_monthly.';

CREATE TABLE shakedown.usage_monthly (
  org_id   uuid   NOT NULL REFERENCES identity.orgs(id) ON DELETE CASCADE,
  month    date   NOT NULL CHECK (extract(day FROM month) = 1),
  meter    text   NOT NULL,
  quantity bigint NOT NULL,
  PRIMARY KEY (org_id, month, meter)
);
COMMENT ON TABLE shakedown.usage_monthly IS 'Monthly totals, upserted in the same transaction as each usage row; quota checks read this.';

-- 5. Privileges (schema.sql §6, shakedown part).
GRANT USAGE ON SCHEMA shakedown TO shaked_app, shaked_system;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA shakedown TO shaked_app, shaked_system;
REVOKE INSERT, UPDATE, DELETE ON shakedown.quotas FROM shaked_app;   -- staff set quotas (system)
REVOKE UPDATE, DELETE ON shakedown.usage FROM shaked_app;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA shakedown TO shaked_app, shaked_system;

-- 6. Row-level security (schema.sql §7, shakedown part): enabled and forced on every table, with a restrictive
--    org wall; root resources are visible by org role, team seat, authorship or a grant.
DO $$
DECLARE t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['provider_keys','connections','worlds','org_settings','experiments','suites','runs','branches',
                           'events','suite_runs','reviews','saves','sandboxes','api_tokens','run_tokens','grants',
                           'quotas','usage','usage_monthly'] LOOP
    EXECUTE format('ALTER TABLE shakedown.%I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE shakedown.%I FORCE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY org_wall ON shakedown.%I AS RESTRICTIVE FOR ALL TO shaked_app '
                   'USING (org_id = authz.org_id()) WITH CHECK (org_id = authz.org_id())', t);
  END LOOP;
END $$;

DO $$
DECLARE
  r record;
BEGIN
  FOR r IN SELECT * FROM (VALUES ('runs','run_id'), ('experiments','experiment_id'), ('suites','suite_id'),
                                 ('worlds','world_id'), ('provider_keys','provider_key_id'),
                                 ('connections','connection_id')) AS v(tbl, col) LOOP
    EXECUTE format($f$
      CREATE POLICY visible ON shakedown.%1$I FOR SELECT TO shaked_app USING (
        authz.sees(team_id, created_by)
        OR EXISTS (SELECT 1 FROM shakedown.grants g
                   WHERE g.%2$I = %1$I.id
                     AND (g.grantee_user_id = authz.user_id() OR g.grantee_team_id = ANY (authz.team_ids()))
                     AND authz.org_role() IN ('owner','admin','member','viewer')))$f$, r.tbl, r.col);
    EXECUTE format('CREATE POLICY placed ON shakedown.%I FOR INSERT TO shaked_app WITH CHECK (authz.places(team_id))', r.tbl);
    EXECUTE format($f$
      CREATE POLICY changed ON shakedown.%1$I FOR UPDATE TO shaked_app
        USING (authz.writer() AND (authz.sees(team_id, created_by)
               OR EXISTS (SELECT 1 FROM shakedown.grants g WHERE g.%2$I = %1$I.id AND g.level IN ('edit','admin')
                          AND (g.grantee_user_id = authz.user_id() OR g.grantee_team_id = ANY (authz.team_ids())))))
        WITH CHECK (authz.writer())$f$, r.tbl, r.col);
    EXECUTE format($f$
      CREATE POLICY removed ON shakedown.%1$I FOR DELETE TO shaked_app
        USING (authz.writer() AND (authz.sees(team_id, created_by)
               OR EXISTS (SELECT 1 FROM shakedown.grants g WHERE g.%2$I = %1$I.id AND g.level = 'admin'
                          AND (g.grantee_user_id = authz.user_id() OR g.grantee_team_id = ANY (authz.team_ids())))))$f$,
      r.tbl, r.col);
  END LOOP;
END $$;

-- Child and per-org tables: the org wall plus one permissive policy (a table with only a restrictive policy
-- would refuse everything). Parent visibility for these is checked in code.
CREATE POLICY in_org ON shakedown.branches      FOR ALL TO shaked_app USING (true) WITH CHECK (true);
CREATE POLICY in_org ON shakedown.events        FOR ALL TO shaked_app USING (true) WITH CHECK (true);
CREATE POLICY in_org ON shakedown.suite_runs    FOR ALL TO shaked_app USING (true) WITH CHECK (true);
CREATE POLICY in_org ON shakedown.reviews       FOR ALL TO shaked_app USING (true) WITH CHECK (true);
CREATE POLICY in_org ON shakedown.saves         FOR ALL TO shaked_app USING (true) WITH CHECK (true);
CREATE POLICY in_org ON shakedown.sandboxes     FOR ALL TO shaked_app USING (true) WITH CHECK (true);
CREATE POLICY in_org ON shakedown.run_tokens    FOR ALL TO shaked_app USING (true) WITH CHECK (true);
CREATE POLICY in_org ON shakedown.usage         FOR ALL TO shaked_app USING (true) WITH CHECK (true);
CREATE POLICY in_org ON shakedown.usage_monthly FOR ALL TO shaked_app USING (true) WITH CHECK (true);
CREATE POLICY in_org ON shakedown.quotas        FOR SELECT TO shaked_app USING (true);
CREATE POLICY in_org_read ON shakedown.org_settings FOR SELECT TO shaked_app USING (true);
CREATE POLICY admin_write ON shakedown.org_settings FOR ALL TO shaked_app USING (authz.is_admin()) WITH CHECK (authz.is_admin());
CREATE POLICY in_org_read ON shakedown.grants FOR SELECT TO shaked_app USING (true);
CREATE POLICY writer ON shakedown.grants FOR ALL TO shaked_app USING (authz.writer()) WITH CHECK (authz.writer());
-- API tokens: your own, or every token of the org if you administer it.
CREATE POLICY mine ON shakedown.api_tokens FOR ALL TO shaked_app
  USING (user_id = authz.user_id() OR authz.is_admin())
  WITH CHECK (user_id = authz.user_id() OR (service_account_id IS NOT NULL AND authz.is_admin()));
