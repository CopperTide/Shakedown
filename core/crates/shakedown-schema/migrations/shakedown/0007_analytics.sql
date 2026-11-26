-- shakedown 7: analytics. `branches.stats` keeps what an ended branch came to (its world, scenario, setup, model,
-- time, tokens and cost, and the end they were worked out for), worked out once by shaked from its events so the
-- Analytics summary aggregates small rows instead of reading every event. Nullable and without a default: adding
-- it rewrites nothing, and shaked's analytics job fills in every ended branch, old ones included.
ALTER TABLE shakedown.branches
  ADD COLUMN stats jsonb;
COMMENT ON COLUMN shakedown.branches.stats IS 'What the ended branch came to, for analytics (shaked''s analytics job writes it); null until then.';
