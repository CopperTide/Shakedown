-- Step O1 of the hosted M1 plan: take a database that a pre-0c7acdd `factory` shaked migrated (identity 1–2,
-- shakedown 2–3: organisations in open source) back to the base schema (shakedown 1). Keeps every run, branch,
-- event, suite, experiment, review and connection. One transaction: on any error nothing changes.
-- Run as a superuser, after a fresh dump:
--   psql "$URL/<db>" -v ON_ERROR_STOP=1 -f core/scripts/o1-retire-factory-orgs.sql
BEGIN;

-- Row-level security and its policies on the old tables.
DO $$
DECLARE t text; p record;
BEGIN
  FOREACH t IN ARRAY ARRAY['runs','branches','events','suites','suite_runs','experiments','reviews','connections'] LOOP
    FOR p IN SELECT policyname FROM pg_policies WHERE schemaname = 'shakedown' AND tablename = t LOOP
      EXECUTE format('DROP POLICY %I ON shakedown.%I', p.policyname, t);
    END LOOP;
    EXECUTE format('ALTER TABLE shakedown.%I NO FORCE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE shakedown.%I DISABLE ROW LEVEL SECURITY', t);
  END LOOP;
END $$;

-- The tables the retired shakedown 2 created.
DROP TABLE IF EXISTS shakedown.grants, shakedown.run_tokens, shakedown.api_tokens, shakedown.sandboxes,
  shakedown.saves, shakedown.usage, shakedown.usage_monthly, shakedown.quotas, shakedown.org_settings,
  shakedown.provider_keys, shakedown.worlds CASCADE;

-- Its indexes, and the columns it added (their constraints go with them).
DROP INDEX IF EXISTS shakedown.branches_run, shakedown.runs_org_created, shakedown.runs_team,
  shakedown.branches_running, shakedown.suites_experiment;
ALTER TABLE shakedown.runs DROP COLUMN IF EXISTS org_id CASCADE, DROP COLUMN IF EXISTS team_id CASCADE,
  DROP COLUMN IF EXISTS created_by CASCADE, DROP COLUMN IF EXISTS world_id CASCADE, DROP COLUMN IF EXISTS placement CASCADE;
ALTER TABLE shakedown.branches DROP COLUMN IF EXISTS org_id CASCADE, DROP COLUMN IF EXISTS created_by CASCADE;
ALTER TABLE shakedown.events DROP COLUMN IF EXISTS org_id CASCADE;
ALTER TABLE shakedown.experiments DROP COLUMN IF EXISTS org_id CASCADE, DROP COLUMN IF EXISTS team_id CASCADE,
  DROP COLUMN IF EXISTS created_by CASCADE;
ALTER TABLE shakedown.suites DROP COLUMN IF EXISTS org_id CASCADE, DROP COLUMN IF EXISTS team_id CASCADE,
  DROP COLUMN IF EXISTS created_by CASCADE;
ALTER TABLE shakedown.suite_runs DROP COLUMN IF EXISTS org_id CASCADE;
ALTER TABLE shakedown.reviews DROP COLUMN IF EXISTS org_id CASCADE, DROP COLUMN IF EXISTS created_by CASCADE;
ALTER TABLE shakedown.connections DROP COLUMN IF EXISTS org_id CASCADE, DROP COLUMN IF EXISTS team_id CASCADE,
  DROP COLUMN IF EXISTS created_by CASCADE, DROP COLUMN IF EXISTS sealed_env CASCADE;

-- The identity stream's schemas (users, orgs, memberships, teams, audit log, the authz functions).
DROP SCHEMA IF EXISTS identity CASCADE;
DROP SCHEMA IF EXISTS authz CASCADE;

-- The retired history rows; the next shaked applies the new shakedown 2.
DELETE FROM schema_meta.migrations WHERE stream = 'identity' OR (stream = 'shakedown' AND version >= 2);

-- The shaked roles' grants on the base tables (they stay as roles; the cloud uses them).
DO $$
BEGIN
  IF to_regrole('shaked_app') IS NOT NULL THEN
    EXECUTE 'REVOKE ALL ON ALL TABLES IN SCHEMA shakedown FROM shaked_app';
    EXECUTE 'REVOKE ALL ON SCHEMA shakedown FROM shaked_app';
  END IF;
  IF to_regrole('shaked_system') IS NOT NULL THEN
    EXECUTE 'REVOKE ALL ON ALL TABLES IN SCHEMA shakedown FROM shaked_system';
    EXECUTE 'REVOKE ALL ON SCHEMA shakedown FROM shaked_system';
  END IF;
END $$;

COMMIT;
