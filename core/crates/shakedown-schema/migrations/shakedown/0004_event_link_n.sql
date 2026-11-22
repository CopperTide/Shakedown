-- shakedown 4: an event a sandbox's worker sent keeps the worker's own counter `n`, so an event it sends again
-- (after its link dropped, or the control box restarted) is acknowledged with the seq it got instead of being
-- appended twice. Nullable and without a default: adding it rewrites nothing; the events shaked appends itself
-- leave it null, and nulls never collide in the unique index.
ALTER TABLE shakedown.events
  ADD COLUMN link_n bigint;
CREATE UNIQUE INDEX events_branch_link_n ON shakedown.events (branch_id, link_n);
COMMENT ON COLUMN shakedown.events.link_n IS 'The worker''s counter n of an event a sandbox''s worker sent; null otherwise.';
