-- shakedown 9: Explore. `call_usage` keeps one lean row per model call (tokens, cost, think time; never a body),
-- written by shaked's analytics job with each ended branch's figures, so neither Explore nor SQL reads a model call's
-- body (most of a long run's record). `saved_views` keeps named Explore views. Schema `explore` holds the read-only
-- views the SQL tab may query (docs/analytics.md); each reads a branch through its run, so a hosting layer's
-- policies on runs apply. Role `shakedown_explore`, made when the migrating role may, can read those
-- views and nothing else: shaked runs SQL as it when it can. Personal use only: nothing here names an owner.
CREATE TABLE shakedown.call_usage (
  branch_id        text        NOT NULL REFERENCES shakedown.branches(id) ON DELETE CASCADE,
  seq              bigint      NOT NULL,
  at               timestamptz NOT NULL,
  agent            text,
  provider         text,
  model            text,
  input_tokens     bigint,
  output_tokens    bigint,
  cached_tokens    bigint,
  reasoning_tokens bigint,
  cost_usd         double precision,
  think_ms         double precision,
  PRIMARY KEY (branch_id, seq)
);
COMMENT ON TABLE shakedown.call_usage IS 'One row per model call of an ended branch: usage, cost and think time (shaked''s analytics job writes it).';

CREATE TABLE shakedown.saved_views (
  id         text        PRIMARY KEY,
  name       text        NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
  kind       text        NOT NULL CHECK (kind IN ('builder', 'sql')),
  spec       jsonb       NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX saved_views_updated ON shakedown.saved_views (updated_at DESC);
COMMENT ON TABLE shakedown.saved_views IS 'Named Explore views: a builder spec or an SQL query, with how to chart it.';

-- A tool call's tool; null for other steps.
CREATE FUNCTION shakedown.step_tool(kind text, payload jsonb) RETURNS text LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
  SELECT CASE WHEN kind = 'tool_call' THEN coalesce(nullif(payload->>'name', ''), 'tool') END
$$;
-- How a tool call or a turn came out: ok, refused (a gate or a decision right held it back) or error; null for other
-- steps. The run page's agent inspector tells them apart the same way.
CREATE FUNCTION shakedown.step_outcome(kind text, payload jsonb) RETURNS text LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
  SELECT CASE
    WHEN kind = 'tool_call' THEN CASE
      WHEN coalesce(payload->>'error', '') = '' THEN 'ok'
      WHEN payload->>'error' ~* '\mrefused\M|not available to this agent' THEN 'refused'
      ELSE 'error' END
    WHEN kind IN ('idle', 'agent_end') THEN CASE WHEN payload->'ok' = 'false'::jsonb THEN 'error' ELSE 'ok' END
  END
$$;

CREATE SCHEMA explore;
COMMENT ON SCHEMA explore IS 'Shakedown Explore: read-only views over runs, steps, model calls, tool calls and agents.';

CREATE VIEW explore.runs AS
SELECT r.id AS run_id, b.id AS branch_id, b.parent_id AS parent_branch_id, coalesce(r.name, r.task_id) AS name,
       ws.world, r.task_id AS scenario, b.team->>'name' AS setup, b.stats->>'model' AS model,
       x.experiment_id, x.experiment, b.status,
       CASE WHEN b.status IN ('passed', 'failed', 'budget_exceeded') THEN b.status = 'passed' END AS passed,
       b.created_at AS started_at, b.ended_at, extract(epoch FROM b.ended_at - b.created_at)::float8 AS seconds,
       CASE WHEN jsonb_typeof(b.stats->'cost_usd') = 'number' THEN (b.stats->>'cost_usd')::float8 END AS cost_usd,
       CASE WHEN jsonb_typeof(b.stats->'tokens') = 'number' THEN (b.stats->>'tokens')::bigint END AS tokens
  FROM shakedown.branches b
  JOIN shakedown.runs r ON r.id = b.run_id
  LEFT JOIN LATERAL (SELECT nullif(e.payload->>'benchmark', '') AS world FROM shakedown.events e
                      WHERE e.branch_id = b.id AND e.kind = 'run_start' ORDER BY e.seq LIMIT 1) ws ON true
  LEFT JOIN LATERAL (SELECT x.id AS experiment_id, x.name AS experiment
                       FROM shakedown.suite_runs sr JOIN shakedown.suites s ON s.id = sr.suite_id
                       JOIN shakedown.experiments x ON x.id = s.experiment_id
                      WHERE sr.run_id = r.id LIMIT 1) x ON true;

CREATE VIEW explore.steps AS
SELECT b.run_id, e.branch_id, e.seq AS step, e.at, e.agent, e.kind,
       shakedown.step_tool(e.kind, e.payload) AS tool, shakedown.step_outcome(e.kind, e.payload) AS outcome,
       left(shakedown.step_text(e.kind, e.payload), 500) AS text, e.seq < coalesce(b.start_seq, 0) AS inherited
  FROM shakedown.events e
  JOIN shakedown.branches b ON b.id = e.branch_id
  JOIN shakedown.runs r ON r.id = b.run_id
 WHERE e.kind NOT IN ('world_view', 'save_point', 'spans');

CREATE VIEW explore.model_calls AS
SELECT b.run_id, u.branch_id, u.seq AS step, u.at, u.agent, u.provider, u.model, u.input_tokens, u.output_tokens,
       u.cached_tokens, u.reasoning_tokens, u.cost_usd, u.think_ms, u.seq < coalesce(b.start_seq, 0) AS inherited
  FROM shakedown.call_usage u
  JOIN shakedown.branches b ON b.id = u.branch_id
  JOIN shakedown.runs r ON r.id = b.run_id;

CREATE VIEW explore.tool_calls AS
SELECT b.run_id, e.branch_id, e.seq AS step, e.at, e.agent, shakedown.step_tool(e.kind, e.payload) AS tool,
       shakedown.step_outcome(e.kind, e.payload) AS outcome, left(nullif(e.payload->>'error', ''), 500) AS error,
       CASE WHEN jsonb_typeof(e.payload->'duration_ms') = 'number' THEN (e.payload->>'duration_ms')::float8 END AS duration_ms,
       e.payload->>'part' AS part, e.seq < coalesce(b.start_seq, 0) AS inherited
  FROM shakedown.events e
  JOIN shakedown.branches b ON b.id = e.branch_id
  JOIN shakedown.runs r ON r.id = b.run_id
 WHERE e.kind = 'tool_call';

CREATE VIEW explore.agents AS
SELECT b.run_id, b.id AS branch_id, a->>'id' AS agent, a->>'name' AS name, a->>'kind' AS kind, a->>'model' AS model,
       coalesce(u.model_calls, 0) AS model_calls, coalesce(u.input_tokens, 0) AS input_tokens,
       coalesce(u.output_tokens, 0) AS output_tokens, coalesce(u.cached_tokens, 0) AS cached_tokens,
       u.cost_usd, coalesce(u.think_ms, 0) / 1000.0 AS think_seconds
  FROM shakedown.branches b
  JOIN shakedown.runs r ON r.id = b.run_id
 CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_typeof(b.team->'agents') = 'array'
                                              THEN b.team->'agents' ELSE '[]'::jsonb END) a
  LEFT JOIN LATERAL (SELECT count(*) AS model_calls, sum(c.input_tokens)::bigint AS input_tokens,
                            sum(c.output_tokens)::bigint AS output_tokens, sum(c.cached_tokens)::bigint AS cached_tokens,
                            sum(c.cost_usd) AS cost_usd, sum(c.think_ms) AS think_ms
                       FROM shakedown.call_usage c
                      WHERE c.branch_id = b.id AND c.agent = a->>'id' AND c.seq >= coalesce(b.start_seq, 0)) u ON true;

COMMENT ON VIEW explore.runs IS 'One row per run branch: a run''s first branch, or a rerun or fork of it (parent_branch_id set).';
COMMENT ON COLUMN explore.runs.run_id IS 'The run.';
COMMENT ON COLUMN explore.runs.branch_id IS 'This branch: what steps, model_calls, tool_calls and agents join on.';
COMMENT ON COLUMN explore.runs.parent_branch_id IS 'The branch it was forked or rerun from; null for a run''s first branch.';
COMMENT ON COLUMN explore.runs.name IS 'The run''s name, else its scenario.';
COMMENT ON COLUMN explore.runs.world IS 'The world it ran in.';
COMMENT ON COLUMN explore.runs.scenario IS 'The scenario (task) it attempted.';
COMMENT ON COLUMN explore.runs.setup IS 'The agent setup''s name.';
COMMENT ON COLUMN explore.runs.model IS 'The model that used the most tokens; null until its figures are worked out.';
COMMENT ON COLUMN explore.runs.experiment_id IS 'The experiment it belongs to, if any.';
COMMENT ON COLUMN explore.runs.experiment IS 'That experiment''s name.';
COMMENT ON COLUMN explore.runs.status IS 'running, passed, failed, budget_exceeded, stopped, interrupted or infra_error.';
COMMENT ON COLUMN explore.runs.passed IS 'True or false for a judged run (passed, failed, over budget); null otherwise.';
COMMENT ON COLUMN explore.runs.started_at IS 'When it started.';
COMMENT ON COLUMN explore.runs.ended_at IS 'When it ended; null while it runs.';
COMMENT ON COLUMN explore.runs.seconds IS 'Wall-clock seconds from start to end; null while it runs.';
COMMENT ON COLUMN explore.runs.cost_usd IS 'What its model calls cost in USD; null until worked out, or when some cost is unknown.';
COMMENT ON COLUMN explore.runs.tokens IS 'Tokens its agents used; null until worked out.';

COMMENT ON VIEW explore.steps IS 'One row per step of a run branch (bookkeeping left out), with a short text; never a model call''s body.';
COMMENT ON COLUMN explore.steps.run_id IS 'The run.';
COMMENT ON COLUMN explore.steps.branch_id IS 'The branch.';
COMMENT ON COLUMN explore.steps.step IS 'The step number in the branch (the run page''s ?step=).';
COMMENT ON COLUMN explore.steps.at IS 'When it was recorded.';
COMMENT ON COLUMN explore.steps.agent IS 'The agent that took it; null for the world''s own steps.';
COMMENT ON COLUMN explore.steps.kind IS 'run_start, agent_start, model_call, reasoning, tool_call, message, delegation, report, wake, idle, world_event, check, agent_end, finish, ...';
COMMENT ON COLUMN explore.steps.tool IS 'A tool call''s tool; null for other steps.';
COMMENT ON COLUMN explore.steps.outcome IS 'ok, refused or error for a tool call or a turn''s end; null for other steps.';
COMMENT ON COLUMN explore.steps.text IS 'What it said or did, up to 500 characters (reasoning, messages, tool calls, checks); null for model calls and world events.';
COMMENT ON COLUMN explore.steps.inherited IS 'True for a fork''s steps copied from the branch it was forked from.';

COMMENT ON VIEW explore.model_calls IS 'One row per model call of an ended branch: usage, cost and think time only (no prompt or answer).';
COMMENT ON COLUMN explore.model_calls.run_id IS 'The run.';
COMMENT ON COLUMN explore.model_calls.branch_id IS 'The branch.';
COMMENT ON COLUMN explore.model_calls.step IS 'The step it was recorded at.';
COMMENT ON COLUMN explore.model_calls.at IS 'When its answer came back.';
COMMENT ON COLUMN explore.model_calls.agent IS 'The agent that made it.';
COMMENT ON COLUMN explore.model_calls.provider IS 'The provider that answered (openrouter, anthropic, openai, a local server, ...).';
COMMENT ON COLUMN explore.model_calls.model IS 'The model asked.';
COMMENT ON COLUMN explore.model_calls.input_tokens IS 'The whole prompt in tokens, cached tokens included.';
COMMENT ON COLUMN explore.model_calls.output_tokens IS 'Output tokens.';
COMMENT ON COLUMN explore.model_calls.cached_tokens IS 'Prompt tokens read from the provider''s cache.';
COMMENT ON COLUMN explore.model_calls.reasoning_tokens IS 'Output tokens spent reasoning, when the provider says.';
COMMENT ON COLUMN explore.model_calls.cost_usd IS 'What it cost in USD: as reported, else at list price; null when unknown.';
COMMENT ON COLUMN explore.model_calls.think_ms IS 'Milliseconds since the agent''s previous step (an estimate of the call''s time); null for its first.';
COMMENT ON COLUMN explore.model_calls.inherited IS 'True for a fork''s calls copied from the branch it was forked from.';

COMMENT ON VIEW explore.tool_calls IS 'One row per tool call of a run branch.';
COMMENT ON COLUMN explore.tool_calls.run_id IS 'The run.';
COMMENT ON COLUMN explore.tool_calls.branch_id IS 'The branch.';
COMMENT ON COLUMN explore.tool_calls.step IS 'The step it was recorded at.';
COMMENT ON COLUMN explore.tool_calls.at IS 'When it returned.';
COMMENT ON COLUMN explore.tool_calls.agent IS 'The agent that called it.';
COMMENT ON COLUMN explore.tool_calls.tool IS 'The tool.';
COMMENT ON COLUMN explore.tool_calls.outcome IS 'ok, refused (a gate or decision right held it back) or error.';
COMMENT ON COLUMN explore.tool_calls.error IS 'Its error, up to 500 characters; null when it returned fine.';
COMMENT ON COLUMN explore.tool_calls.duration_ms IS 'How long the world took, in milliseconds; null when not measured.';
COMMENT ON COLUMN explore.tool_calls.part IS 'The world part (system) that served it.';
COMMENT ON COLUMN explore.tool_calls.inherited IS 'True for a fork''s calls copied from the branch it was forked from.';

COMMENT ON VIEW explore.agents IS 'One row per agent of a run branch''s setup, with its own model calls'' totals.';
COMMENT ON COLUMN explore.agents.run_id IS 'The run.';
COMMENT ON COLUMN explore.agents.branch_id IS 'The branch.';
COMMENT ON COLUMN explore.agents.agent IS 'The agent''s id (steps.agent).';
COMMENT ON COLUMN explore.agents.name IS 'Its name.';
COMMENT ON COLUMN explore.agents.kind IS 'open, claude_code, codex, http, external, ...';
COMMENT ON COLUMN explore.agents.model IS 'The model its setup gives it.';
COMMENT ON COLUMN explore.agents.model_calls IS 'Its model calls in this branch (inherited ones left out).';
COMMENT ON COLUMN explore.agents.input_tokens IS 'Their prompt tokens.';
COMMENT ON COLUMN explore.agents.output_tokens IS 'Their output tokens.';
COMMENT ON COLUMN explore.agents.cached_tokens IS 'Their cached prompt tokens.';
COMMENT ON COLUMN explore.agents.cost_usd IS 'What they cost in USD (calls of unknown cost left out); null without a known cost.';
COMMENT ON COLUMN explore.agents.think_seconds IS 'Their estimated think time in seconds.';

-- The role SQL runs as: reads the explore views, nothing else (the views read the tables as their owner). Made only
-- when the migrating role may make roles; roles are shared by every database of the server, so several databases
-- (or tests) may race to make it.
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'shakedown_explore') THEN
    BEGIN
      CREATE ROLE shakedown_explore NOLOGIN;
    EXCEPTION WHEN duplicate_object OR unique_violation OR insufficient_privilege THEN NULL;
    END;
  END IF;
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'shakedown_explore') THEN
    BEGIN
      GRANT USAGE ON SCHEMA explore TO shakedown_explore;
      GRANT SELECT ON ALL TABLES IN SCHEMA explore TO shakedown_explore;
    EXCEPTION WHEN insufficient_privilege THEN NULL;
    END;
  END IF;
END $$;
