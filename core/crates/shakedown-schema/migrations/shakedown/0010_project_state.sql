-- shakedown 10: a project's state in typed tables (plan 2026-11-27, "Project state in databases"). `projects` holds a
-- project's settings (today's shakedown.yaml). Scenarios, setups (with their MADL files) and experiments are rows
-- whose `body` is the item exactly as its file says, `format:` included, so export and import round-trip; references
-- between items stay names inside the body, with no foreign keys. Each row has a revision, bumped by every write, so
-- a write based on an older one is refused; a delete only sets `deleted_at`. `project_changes` with their items
-- (before and after) are the undo history that replaces `.shakedown/trash`; `project_imports` replaces each git
-- import's import.json. A local shaked keeps scenarios and setups as files, so those two tables stay empty there:
-- they exist so one schema serves a hosting layer that keeps them as rows too. Nothing reads these tables yet.
-- Personal use only: nothing here names an owner; a hosting layer adds its own columns and keys on top.
CREATE TABLE shakedown.projects (
  id                 uuid             PRIMARY KEY DEFAULT gen_random_uuid(),
  name               text             NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
  default_world      text             CHECK (length(default_world) <= 64),
  cost_cap_usd       double precision CHECK (cost_cap_usd >= 0),
  edit_policy        text             NOT NULL DEFAULT 'diff_first' CHECK (edit_policy IN ('diff_first','apply_then_tell')),
  spend_policy       text             NOT NULL DEFAULT 'always_ask' CHECK (spend_policy IN ('always_ask','under_cap')),
  spend_usd_per_day  double precision CHECK (spend_usd_per_day >= 0),
  assistant_model    text             CHECK (length(assistant_model) BETWEEN 1 AND 200),
  assistant_provider text             CHECK (length(assistant_provider) BETWEEN 1 AND 100),
  revision           bigint           NOT NULL DEFAULT 1 CHECK (revision >= 1),
  created_at         timestamptz      NOT NULL DEFAULT now(),
  updated_at         timestamptz      NOT NULL DEFAULT now(),
  CHECK ((spend_policy = 'under_cap') = (spend_usd_per_day IS NOT NULL)),
  CHECK (assistant_provider IS NULL OR assistant_model IS NOT NULL)
);
COMMENT ON TABLE shakedown.projects IS 'A project''s settings (shakedown.yaml): name, defaults, permissions, assistant model.';

CREATE TABLE shakedown.project_scenarios (
  id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  project_id uuid        NOT NULL REFERENCES shakedown.projects(id) ON DELETE CASCADE,
  slug       text        NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,63}$'),
  body       jsonb       NOT NULL CHECK (jsonb_typeof(body) = 'object' AND octet_length(body::text) <= 4194304),
  label      text        GENERATED ALWAYS AS (body->>'label') STORED,
  world      text        GENERATED ALWAYS AS (body->>'world') STORED,
  revision   bigint      NOT NULL DEFAULT 1 CHECK (revision >= 1),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz
);
CREATE UNIQUE INDEX project_scenarios_slug ON shakedown.project_scenarios (project_id, slug) WHERE deleted_at IS NULL;
COMMENT ON TABLE shakedown.project_scenarios IS 'A project''s scenarios, each as its file (scenarios/<slug>.yaml) says.';

CREATE TABLE shakedown.project_setups (
  id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  project_id uuid        NOT NULL REFERENCES shakedown.projects(id) ON DELETE CASCADE,
  slug       text        NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,63}$'),
  body       jsonb       NOT NULL CHECK (jsonb_typeof(body) = 'object' AND octet_length(body::text) <= 4194304),
  label      text        GENERATED ALWAYS AS (body->>'label') STORED,
  team_kind  text        GENERATED ALWAYS AS (body->'team'->>'kind') STORED,
  revision   bigint      NOT NULL DEFAULT 1 CHECK (revision >= 1),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz
);
CREATE UNIQUE INDEX project_setups_slug ON shakedown.project_setups (project_id, slug) WHERE deleted_at IS NULL;
COMMENT ON TABLE shakedown.project_setups IS 'A project''s agent setups, each as its file (setups/<slug>.yaml) says.';

-- A setup's MADL files (setups/<slug>/<path>), kept as written; a change to one bumps its setup's revision.
CREATE TABLE shakedown.project_setup_files (
  setup_id   uuid        NOT NULL REFERENCES shakedown.project_setups(id) ON DELETE CASCADE,
  path       text        NOT NULL CHECK (length(path) <= 1024 AND path ~ '^[^/\\]+(/[^/\\]+)*$'
                                         AND path !~ '(^|/)\.\.?(/|$)'),
  body       text        NOT NULL CHECK (octet_length(body) <= 4194304),
  updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (setup_id, path)
);
COMMENT ON TABLE shakedown.project_setup_files IS 'The files of a setup described in MADL, by path in its folder.';

CREATE TABLE shakedown.project_experiments (
  id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  project_id uuid        NOT NULL REFERENCES shakedown.projects(id) ON DELETE CASCADE,
  slug       text        NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,63}$'),
  body       jsonb       NOT NULL CHECK (jsonb_typeof(body) = 'object' AND octet_length(body::text) <= 4194304),
  label      text        GENERATED ALWAYS AS (body->>'label') STORED,
  world      text        GENERATED ALWAYS AS (body->>'world') STORED,
  scenario   text        GENERATED ALWAYS AS (body->>'scenario') STORED,
  parent     text        GENERATED ALWAYS AS (body->>'parent') STORED,
  revision   bigint      NOT NULL DEFAULT 1 CHECK (revision >= 1),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz
);
CREATE UNIQUE INDEX project_experiments_slug ON shakedown.project_experiments (project_id, slug) WHERE deleted_at IS NULL;
COMMENT ON TABLE shakedown.project_experiments IS 'A project''s experiment designs, each as its file (experiments/<slug>.yaml) says.';

-- A deployed experiment names the design it came from by row; `project_experiment` (shakedown 3) names it by slug
-- until the slug link is retired.
ALTER TABLE shakedown.experiments
  ADD COLUMN project_experiment_id uuid REFERENCES shakedown.project_experiments(id) ON DELETE SET NULL;
CREATE INDEX experiments_project_experiment_id_idx ON shakedown.experiments (project_experiment_id, created_at DESC);
COMMENT ON COLUMN shakedown.experiments.project_experiment_id IS 'The project experiment it was deployed from, if any.';

-- One change to a project (an edit, a delete with what it cascaded to, an import, an undo), newest kept 30 days and
-- always the last 20. Undo puts each item's `before` back, refusing when the item moved past `revision` since.
CREATE TABLE shakedown.project_changes (
  id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  project_id uuid        NOT NULL REFERENCES shakedown.projects(id) ON DELETE CASCADE,
  label      text        NOT NULL CHECK (length(label) BETWEEN 1 AND 200),   -- e.g. "delete world mill"
  undone_at  timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX project_changes_recent ON shakedown.project_changes (project_id, created_at DESC);
COMMENT ON TABLE shakedown.project_changes IS 'A project''s undo history: one row per change, its items in project_change_items.';

-- An item a change touched: a row (the project's settings, a scenario, setup or experiment: `before` and `after` are
-- its body), a setup's MADL file, or a file of the project folder (a world's files; a local scenario or setup). A
-- file's bytes are in `before_bytes` and `after_bytes`. Null before: it was made; null after: it was deleted.
CREATE TABLE shakedown.project_change_items (
  change_id    uuid    NOT NULL REFERENCES shakedown.project_changes(id) ON DELETE CASCADE,
  n            integer NOT NULL CHECK (n >= 0),
  kind         text    NOT NULL CHECK (kind IN ('project','scenario','setup','experiment','setup_file','file')),
  item_id      uuid,                                       -- the row; for a setup file its setup; null for a file
  name         text    NOT NULL CHECK (length(name) BETWEEN 1 AND 1024),   -- the slug, or the file's path
  before       jsonb,
  after        jsonb,
  before_bytes bytea   CHECK (octet_length(before_bytes) <= 10485760),
  after_bytes  bytea   CHECK (octet_length(after_bytes) <= 10485760),
  revision     bigint  CHECK (revision >= 1),              -- the row's revision this change left
  PRIMARY KEY (change_id, n),
  CHECK ((kind = 'file') = (item_id IS NULL)),
  CHECK (kind IN ('setup_file','file') OR (before_bytes IS NULL AND after_bytes IS NULL)),
  CHECK (kind NOT IN ('setup_file','file') OR (before IS NULL AND after IS NULL))
);
COMMENT ON TABLE shakedown.project_change_items IS 'What one change did to each item: before and after.';

-- One git import into a project (the record each re-import updates); its checkouts stay on disk as a cache.
CREATE TABLE shakedown.project_imports (
  id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  project_id  uuid        NOT NULL REFERENCES shakedown.projects(id) ON DELETE CASCADE,
  slug        text        NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,63}$'),  -- the import's id
  url         text        NOT NULL CHECK (length(url) BETWEEN 1 AND 2000),
  git_ref     text        NOT NULL CHECK (length(git_ref) <= 250),
  commit_sha  text        NOT NULL CHECK (commit_sha ~ '^[0-9a-f]{1,64}$'),
  imported_at timestamptz NOT NULL,
  packages    jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(packages) = 'array'),
  items       jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(items) = 'array'),
  edits       jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(edits) = 'array'),
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (project_id, slug)
);
COMMENT ON TABLE shakedown.project_imports IS 'A project''s git imports: what each fetched, registered and wrote, with digests.';
