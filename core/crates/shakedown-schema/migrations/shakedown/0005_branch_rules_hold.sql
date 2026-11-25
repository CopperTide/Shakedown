-- shakedown 5: what the control box decided for a branch, kept where a sandbox's worker cannot change it and a
-- restart does not lose it. `rules` is the branch's run rules as the control box planned them (its spend limit
-- among them); `hold` is what it last told the branch's worker: `paused`, `stopping` (the branch ends `stopped`) or
-- `budget_exceeded` (it ends `budget_exceeded`); `paused_at` is when its pause began (a resume extends the worker token
-- by how long it lasted). All nullable and without a default: adding them rewrites nothing, and a branch from before
-- has no rules on record.
ALTER TABLE shakedown.branches
  ADD COLUMN rules jsonb,
  ADD COLUMN hold text CHECK (hold IN ('paused', 'stopping', 'budget_exceeded')),
  ADD COLUMN paused_at timestamptz;
COMMENT ON COLUMN shakedown.branches.rules IS 'The run rules the control box planned the branch with; null for a branch from before.';
COMMENT ON COLUMN shakedown.branches.hold IS 'What the control box last told the branch''s worker: paused, stopping or budget_exceeded; null to go on.';
COMMENT ON COLUMN shakedown.branches.paused_at IS 'When the branch''s pause began, while it is paused; null otherwise.';
