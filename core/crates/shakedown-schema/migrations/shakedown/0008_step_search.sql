-- shakedown 8: search over every run's steps. `step_text` is the text a step is found by: what an agent thought,
-- said, handed off, delegated, reported or finished with, a tool call's name, error, arguments and result, and
-- checks by their names and details. Bookkeeping, world events and model calls (whose text the reasoning steps already carry) give
-- null and are not searchable. The text is capped so one huge tool result cannot overflow a tsvector.
--
-- An expression index, not a stored column: adding it rewrites nothing (a generated column would rewrite every
-- event, model-call bodies included) and model-call payloads are never detoasted to build it. Building it holds
-- inserts into events for as long as it reads the table once (seconds for a few hundred thousand events); shaked
-- migrates before it serves. Queries must use the same expression, `to_tsvector('english', shakedown.step_text(kind,
-- payload))`, for the planner to use the index.
CREATE OR REPLACE FUNCTION shakedown.step_text(kind text, payload jsonb) RETURNS text
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
  SELECT left(CASE kind
    WHEN 'tool_call' THEN coalesce(payload->>'name', '') || ' ' || coalesce(payload->>'error', '') || ' '
      || coalesce(payload->>'args', '') || ' ' || coalesce(payload->>'result', '')
    WHEN 'check' THEN payload->>'checks'
    WHEN 'agent_end' THEN payload->>'error'
    WHEN 'reasoning' THEN payload->>'text'
    WHEN 'message' THEN payload->>'text'
    WHEN 'report' THEN payload->>'text'
    WHEN 'idle' THEN coalesce(payload->>'text', '') || ' ' || coalesce(payload->>'error', '')
    WHEN 'delegation' THEN payload->>'task'
    WHEN 'handoff' THEN payload->>'note'
    WHEN 'wake' THEN payload->>'input'
    WHEN 'finish' THEN payload->>'summary'
  END, 60000)
$$;
COMMENT ON FUNCTION shakedown.step_text(text, jsonb) IS
  'The text a step is found by in search; null for steps search leaves out. events_search indexes it.';

CREATE INDEX IF NOT EXISTS events_search ON shakedown.events
  USING gin (to_tsvector('english'::regconfig, shakedown.step_text(kind, payload)));
