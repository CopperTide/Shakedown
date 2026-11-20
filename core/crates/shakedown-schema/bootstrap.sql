-- CopperTide schema bootstrap: section 0 of docs/superpowers/specs/2026-11-19-schema.sql (CopperTide-Cloud).
-- An operator runs this once per Postgres cluster and database as a superuser, before any migration:
--   psql "$URL" -f core/crates/shakedown-schema/bootstrap.sql
-- A local shaked on a superuser login (the compose Postgres) runs it itself at every start. It is idempotent,
-- also when two databases on one server run it at once (roles are per cluster).
-- Login roles are made per deployment, e.g.
--   CREATE ROLE shaked LOGIN PASSWORD '...' IN ROLE shaked_app;  CREATE ROLE shaked_jobs LOGIN ... IN ROLE shaked_system;
DO $$
DECLARE
  r record;
BEGIN
  FOR r IN SELECT * FROM (VALUES
      ('coppertide_migrator', 'NOLOGIN'),          -- owns every schema object; used only to migrate
      ('gateway_app',         'NOLOGIN'),          -- the Cloud gateway at runtime: identity + cloud
      ('shaked_app',          'NOLOGIN'),          -- shaked request paths: always under RLS
      ('shaked_system',       'NOLOGIN BYPASSRLS') -- shaked background jobs that span orgs
    ) AS v(name, attrs) LOOP
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = r.name) THEN
      BEGIN
        EXECUTE format('CREATE ROLE %I %s', r.name, r.attrs);
      EXCEPTION WHEN duplicate_object OR unique_violation THEN
        NULL; -- another database on this server made it at the same moment
      END;
    END IF;
  END LOOP;
END $$;

CREATE SCHEMA IF NOT EXISTS schema_meta AUTHORIZATION coppertide_migrator;
CREATE SCHEMA IF NOT EXISTS authz       AUTHORIZATION coppertide_migrator;
CREATE SCHEMA IF NOT EXISTS identity    AUTHORIZATION coppertide_migrator;
CREATE SCHEMA IF NOT EXISTS cloud       AUTHORIZATION coppertide_migrator;
CREATE SCHEMA IF NOT EXISTS shakedown   AUTHORIZATION coppertide_migrator;
