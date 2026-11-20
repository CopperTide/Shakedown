-- shakedown 2: your own model keys and OpenAI-compatible servers, built worlds, uploaded saves, API tokens and
-- run tokens; where a run happens (placement) and which built world it uses; a connection's sealed secrets.
-- Personal use only: nothing here names an owner. A clash of names is checked by shaked under
-- pg_advisory_xact_lock(hashtext('<table>:name:' || name)); a cloud layer adds its own columns and keys on top.
ALTER TABLE shakedown.runs
  ADD COLUMN world_id uuid,
  ADD COLUMN placement text NOT NULL DEFAULT 'local' CHECK (placement IN ('local','cloud','record'));
COMMENT ON TABLE shakedown.runs IS 'A run: task, agent team, world. placement: local (this machine), cloud (a sandbox), record (fed from outside).';
ALTER TABLE shakedown.connections
  ADD COLUMN sealed_env bytea;                            -- secret env values, sealed (vault context "connection:<id>:env")
COMMENT ON TABLE shakedown.connections IS 'Code connections (a driver command). A shaked that serves others runs them only in sandboxes.';

CREATE TABLE shakedown.provider_keys (
  id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  name         text        NOT NULL CHECK (length(name) BETWEEN 1 AND 60),   -- unique per owner: checked by shaked
  kind         text        NOT NULL CHECK (kind IN ('anthropic','openai','openrouter','bedrock','custom')),
  base_url     text        CHECK (length(base_url) <= 500),                  -- custom: the server; bedrock: the region
  model        text        CHECK (length(model) <= 100),
  sealed_key   bytea       NOT NULL,                      -- vault context "provider_key:<id>:key"; never returned
  key_last4    text        NOT NULL,
  prices       jsonb       NOT NULL DEFAULT '{}',         -- per-model USD per token, for custom servers
  checked_ok   boolean,
  checked_at   timestamptz,
  check_detail text        NOT NULL DEFAULT '',           -- the provider's message, keys redacted
  created_at   timestamptz NOT NULL DEFAULT now(),
  CHECK (kind <> 'custom' OR base_url IS NOT NULL)
);
COMMENT ON TABLE shakedown.provider_keys IS
  'Your model keys and OpenAI-compatible servers (vLLM, Ollama, a RunPod pod). Write-only: used by shaked''s model gateway, never sent to a browser or an agent.';

CREATE TABLE shakedown.worlds (
  id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  name         text        NOT NULL CHECK (length(name) BETWEEN 1 AND 80),   -- unique per owner: checked by shaked
  package      text        NOT NULL,                      -- the world package it was built from
  version      text        NOT NULL,
  image_digest text,                                      -- the sandbox image it was built for (warm worlds)
  state        text        NOT NULL DEFAULT 'building' CHECK (state IN ('building','ready','failed','deleted')),
  store        text        NOT NULL,                      -- the object store, by name, that holds object_key
  object_key   text,                                      -- <prefix>/worlds/<id>/... (prefix from the layer)
  bytes        bigint      CHECK (bytes >= 0),
  error        text,
  created_at   timestamptz NOT NULL DEFAULT now(),
  built_at     timestamptz
);
COMMENT ON TABLE shakedown.worlds IS 'A built world kept in object storage. Packages shipped with shaked are not rows.';

ALTER TABLE shakedown.runs
  ADD CONSTRAINT runs_world_id_fkey FOREIGN KEY (world_id) REFERENCES shakedown.worlds(id) ON DELETE SET NULL;

CREATE TABLE shakedown.saves (
  id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  run_id      text        NOT NULL REFERENCES shakedown.runs(id) ON DELETE CASCADE,
  branch_id   text        NOT NULL REFERENCES shakedown.branches(id) ON DELETE CASCADE,
  integration text        NOT NULL,                       -- the part/package that made it
  save_id     text        NOT NULL,                       -- the part's own id, as events name it
  seq         bigint      NOT NULL,                       -- the event it was taken at
  store       text        NOT NULL,                       -- the object store, by name; a row never holds a URL
  object_key  text        NOT NULL,                       -- <prefix>/saves/<integration>/<save_id>/
  bytes       bigint      CHECK (bytes >= 0),
  sha256      text,
  state       text        NOT NULL DEFAULT 'uploading' CHECK (state IN ('uploading','ready','deleted')),
  created_at  timestamptz NOT NULL DEFAULT now(),
  ready_at    timestamptz,
  expires_at  timestamptz                                 -- retention: 30 days after the run's last fork (D19)
);
CREATE INDEX saves_part ON shakedown.saves (integration, save_id);    -- not unique: a hosting layer makes it unique per owner
CREATE INDEX saves_branch_seq ON shakedown.saves (branch_id, seq);
CREATE INDEX saves_expiry ON shakedown.saves (expires_at) WHERE state <> 'deleted';
COMMENT ON TABLE shakedown.saves IS 'A world save uploaded to object storage, so a fork can start somewhere else. Fork-ready when state=ready.';

CREATE TABLE shakedown.api_tokens (
  id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  name         text        NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
  token_hash   text        NOT NULL UNIQUE,               -- sha256 hex of "sd_<64 hex>"
  token_prefix text        NOT NULL,                      -- first 8 characters, shown in lists
  scopes       text[]      NOT NULL CHECK (cardinality(scopes) > 0 AND scopes <@ ARRAY['read','run','admin']),
  created_at   timestamptz NOT NULL DEFAULT now(),
  last_used_at timestamptz,                               -- bumped at most once a minute
  expires_at   timestamptz,
  revoked_at   timestamptz
);
COMMENT ON TABLE shakedown.api_tokens IS 'SDK and CLI tokens, shown once, capped by scope: read views, run also starts, forks, stops and records, admin does everything.';

CREATE TABLE shakedown.run_tokens (
  id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  run_id       text        NOT NULL REFERENCES shakedown.runs(id) ON DELETE CASCADE,
  branch_id    text        NOT NULL REFERENCES shakedown.branches(id) ON DELETE CASCADE,
  -- run: register agents, report, finish (sdr_); worker: a sandbox's link for one branch (sdw_);
  -- ticket: one-time, traded for a worker token (sdt_). Agent tokens (sda_) live in shaked's memory only.
  kind         text        NOT NULL CHECK (kind IN ('run','agent','worker','ticket')),
  agent        text,
  sandbox_id   uuid,                                      -- worker and ticket: the sandbox (the cloud adds the FK)
  token_hash   text        NOT NULL UNIQUE,
  created_at   timestamptz NOT NULL DEFAULT now(),
  expires_at   timestamptz NOT NULL,                      -- counts unpaused time: a resume extends it
  used_at      timestamptz,                               -- tickets: single use
  last_used_at timestamptz,
  revoked_at   timestamptz,                               -- the agent or branch ended
  CHECK ((kind = 'agent') = (agent IS NOT NULL)),
  CHECK (kind NOT IN ('worker','ticket') OR sandbox_id IS NOT NULL)
);
CREATE INDEX run_tokens_branch ON shakedown.run_tokens (branch_id) WHERE revoked_at IS NULL;
COMMENT ON TABLE shakedown.run_tokens IS 'Short-lived machine tokens bound to one branch. Never in events, logs or exports.';

CREATE INDEX branches_run ON shakedown.branches (run_id);
CREATE INDEX branches_running ON shakedown.branches (status) WHERE status = 'running';
