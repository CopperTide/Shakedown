-- shakedown 6: where runs are exported (OpenTelemetry), and what was exported. A target is an OTLP/HTTP traces
-- endpoint (Langfuse, Arize Phoenix or any OTLP collector) with its headers sealed (vault context
-- "observability_target:<id>:headers", a JSON object; lists show only the header names). An export is one branch
-- sent to one target: queued when the branch is made (for targets that export automatically), sent once the branch
-- has ended, retried with backoff. Personal use only: nothing here names an owner; a cloud layer adds its own
-- columns and keys on top.
CREATE TABLE shakedown.observability_targets (
  id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  name            text        NOT NULL CHECK (length(name) BETWEEN 1 AND 60),   -- unique per owner: checked by shaked
  kind            text        NOT NULL CHECK (kind IN ('langfuse','phoenix','otlp')),
  endpoint        text        NOT NULL CHECK (length(endpoint) BETWEEN 1 AND 500), -- the OTLP/HTTP traces URL
  project         text        CHECK (length(project) <= 100),                   -- Phoenix's project name
  sealed_headers  bytea,                                  -- never returned
  header_names    text[]      NOT NULL DEFAULT '{}',
  include_content boolean     NOT NULL DEFAULT true,      -- prompts, completions, tool arguments and results
  auto_export     boolean     NOT NULL DEFAULT true,      -- every new branch, once it ends
  export_metrics  boolean     NOT NULL DEFAULT false,     -- OTLP metrics to the same collector
  checked_ok      boolean,
  checked_at      timestamptz,
  check_detail    text        NOT NULL DEFAULT '',
  created_at      timestamptz NOT NULL DEFAULT now()
);
COMMENT ON TABLE shakedown.observability_targets IS
  'Where runs are exported as OpenTelemetry traces. Header values are sealed and write-only: used by shaked''s exporter, never sent to a browser or a sandbox.';

CREATE TABLE shakedown.observability_exports (
  target_id   uuid        NOT NULL REFERENCES shakedown.observability_targets(id) ON DELETE CASCADE,
  branch_id   text        NOT NULL REFERENCES shakedown.branches(id) ON DELETE CASCADE,
  state       text        NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','done','failed')),
  attempts    integer     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  next_at     timestamptz NOT NULL DEFAULT now(),
  spans       integer     CHECK (spans >= 0),
  last_error  text        NOT NULL DEFAULT '' CHECK (length(last_error) <= 500),
  exported_at timestamptz,
  created_at  timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (target_id, branch_id)
);
CREATE INDEX observability_exports_due ON shakedown.observability_exports (next_at) WHERE state = 'pending';
CREATE INDEX observability_exports_branch ON shakedown.observability_exports (branch_id);
COMMENT ON TABLE shakedown.observability_exports IS
  'One branch sent to one observability target: pending until the branch ends and the send succeeds (retried with backoff), then done, or failed after the last attempt.';
