-- shakedown 1: the tables shaked kept in public until now, moved here when present, then shaked's SCHEMA
-- strings from store.rs and store/{suites,experiments,reviews,connections}.rs at bb8e7e8, verbatim and in that
-- order (idempotent, so they also bring an older database's tables up to date).
CREATE SCHEMA IF NOT EXISTS shakedown;

DO $$
DECLARE t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['runs','branches','events','suites','suite_runs','experiments','reviews','connections'] LOOP
    IF to_regclass('public.' || t) IS NOT NULL AND to_regclass('shakedown.' || t) IS NULL THEN
      EXECUTE format('ALTER TABLE public.%I SET SCHEMA shakedown', t);
    END IF;
  END LOOP;
END $$;

SET LOCAL search_path = shakedown;
CREATE TABLE IF NOT EXISTS runs (
    id text PRIMARY KEY,
    task_id text NOT NULL,
    team jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS branches (
    id text PRIMARY KEY,
    run_id text NOT NULL REFERENCES runs(id),
    parent_id text REFERENCES branches(id),
    start_seq bigint,
    team jsonb NOT NULL,
    changes jsonb NOT NULL DEFAULT '[]',
    fork_note text,
    status text NOT NULL DEFAULT 'running',
    error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    ended_at timestamptz
);
CREATE TABLE IF NOT EXISTS events (
    branch_id text NOT NULL REFERENCES branches(id),
    seq bigint NOT NULL,
    at timestamptz NOT NULL DEFAULT clock_timestamp(),
    agent text,
    kind text NOT NULL,
    payload jsonb NOT NULL,
    PRIMARY KEY (branch_id, seq)
);
ALTER TABLE runs ADD COLUMN IF NOT EXISTS name text;
ALTER TABLE branches ADD COLUMN IF NOT EXISTS restore_mode text;
ALTER TABLE branches ADD COLUMN IF NOT EXISTS restored jsonb NOT NULL DEFAULT '[]';
CREATE TABLE IF NOT EXISTS suites (
    id text PRIMARY KEY,
    name text NOT NULL,
    team jsonb NOT NULL,
    tasks jsonb NOT NULL,
    repeats integer NOT NULL,
    runner text NOT NULL,
    benchmark text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    ended_at timestamptz,
    error text
);
CREATE TABLE IF NOT EXISTS suite_runs (
    suite_id text NOT NULL REFERENCES suites(id),
    ord integer NOT NULL,
    task_id text NOT NULL,
    repeat integer NOT NULL,
    claimed boolean NOT NULL DEFAULT false,
    run_id text,
    branch_id text,
    error text,
    PRIMARY KEY (suite_id, ord)
);
ALTER TABLE suites ADD COLUMN IF NOT EXISTS scenario jsonb NOT NULL DEFAULT '{}';
ALTER TABLE suites ADD COLUMN IF NOT EXISTS variants jsonb NOT NULL DEFAULT '[]';
ALTER TABLE suite_runs ADD COLUMN IF NOT EXISTS variant integer NOT NULL DEFAULT 0;
ALTER TABLE suites ADD COLUMN IF NOT EXISTS experiment_id text;
ALTER TABLE suites ADD COLUMN IF NOT EXISTS setup_ord integer;
ALTER TABLE suites ADD COLUMN IF NOT EXISTS max_cost_usd double precision;
ALTER TABLE suites ADD COLUMN IF NOT EXISTS connection text;
ALTER TABLE suites ADD COLUMN IF NOT EXISTS base_seed bigint;
ALTER TABLE suites ADD COLUMN IF NOT EXISTS world_minutes double precision;
CREATE TABLE IF NOT EXISTS experiments (
    id text PRIMARY KEY,
    name text NOT NULL,
    task_id text NOT NULL,
    benchmark text NOT NULL,
    runs integer NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    ended_at timestamptz,
    error text
);
CREATE TABLE IF NOT EXISTS reviews (
    key text PRIMARY KEY,
    kind text NOT NULL,
    status text NOT NULL,
    model text NOT NULL,
    review jsonb,
    issues jsonb NOT NULL DEFAULT '[]',
    error text,
    cost_usd double precision,
    cost_source text,
    tokens bigint,
    seconds double precision,
    digest_bytes bigint,
    created_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz
);
CREATE TABLE IF NOT EXISTS connections (
    id text PRIMARY KEY,
    name text NOT NULL,
    kind text NOT NULL DEFAULT 'code',
    command jsonb NOT NULL,
    workdir text,
    env jsonb NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now()
);
