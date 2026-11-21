-- shakedown 3: the foreign keys shakedown 2 added NOT VALID are checked for the rows that were there, and org_id
-- has no default any more: every insert names its org.
-- contract: unused since 11356c5 (the commit after it, which adds this file, makes every insert name org_id)
--
-- References that today's shaked never cleared are set NULL first, as the new foreign keys' ON DELETE SET NULL
-- would have: a suite slot whose run or branch was deleted, a suite whose experiment is gone.
UPDATE shakedown.suite_runs s SET run_id = NULL
 WHERE run_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM shakedown.runs r WHERE r.org_id = s.org_id AND r.id = s.run_id);
UPDATE shakedown.suite_runs s SET branch_id = NULL
 WHERE branch_id IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM shakedown.branches b WHERE b.org_id = s.org_id AND b.id = s.branch_id);
UPDATE shakedown.suites s SET experiment_id = NULL
 WHERE experiment_id IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM shakedown.experiments e WHERE e.org_id = s.org_id AND e.id = s.experiment_id);

ALTER TABLE shakedown.runs VALIDATE CONSTRAINT runs_org_fkey;
ALTER TABLE shakedown.runs VALIDATE CONSTRAINT runs_team_fkey;
ALTER TABLE shakedown.runs VALIDATE CONSTRAINT runs_created_by_fkey;
ALTER TABLE shakedown.runs VALIDATE CONSTRAINT runs_world_fkey;
ALTER TABLE shakedown.branches VALIDATE CONSTRAINT branches_run_fkey;
ALTER TABLE shakedown.branches VALIDATE CONSTRAINT branches_parent_fkey;
ALTER TABLE shakedown.branches VALIDATE CONSTRAINT branches_created_by_fkey;
ALTER TABLE shakedown.events VALIDATE CONSTRAINT events_branch_fkey;
ALTER TABLE shakedown.experiments VALIDATE CONSTRAINT experiments_org_fkey;
ALTER TABLE shakedown.experiments VALIDATE CONSTRAINT experiments_team_fkey;
ALTER TABLE shakedown.experiments VALIDATE CONSTRAINT experiments_created_by_fkey;
ALTER TABLE shakedown.suites VALIDATE CONSTRAINT suites_org_fkey;
ALTER TABLE shakedown.suites VALIDATE CONSTRAINT suites_team_fkey;
ALTER TABLE shakedown.suites VALIDATE CONSTRAINT suites_created_by_fkey;
ALTER TABLE shakedown.suites VALIDATE CONSTRAINT suites_experiment_fkey;
ALTER TABLE shakedown.suite_runs VALIDATE CONSTRAINT suite_runs_suite_fkey;
ALTER TABLE shakedown.suite_runs VALIDATE CONSTRAINT suite_runs_run_fkey;
ALTER TABLE shakedown.suite_runs VALIDATE CONSTRAINT suite_runs_branch_fkey;
ALTER TABLE shakedown.reviews VALIDATE CONSTRAINT reviews_org_fkey;
ALTER TABLE shakedown.reviews VALIDATE CONSTRAINT reviews_created_by_fkey;
ALTER TABLE shakedown.connections VALIDATE CONSTRAINT connections_org_fkey;
ALTER TABLE shakedown.connections VALIDATE CONSTRAINT connections_team_fkey;
ALTER TABLE shakedown.connections VALIDATE CONSTRAINT connections_created_by_fkey;

ALTER TABLE shakedown.runs        ALTER COLUMN org_id DROP DEFAULT;
ALTER TABLE shakedown.branches    ALTER COLUMN org_id DROP DEFAULT;
ALTER TABLE shakedown.events      ALTER COLUMN org_id DROP DEFAULT;
ALTER TABLE shakedown.experiments ALTER COLUMN org_id DROP DEFAULT;
ALTER TABLE shakedown.suites      ALTER COLUMN org_id DROP DEFAULT;
ALTER TABLE shakedown.suite_runs  ALTER COLUMN org_id DROP DEFAULT;
ALTER TABLE shakedown.reviews     ALTER COLUMN org_id DROP DEFAULT;
ALTER TABLE shakedown.connections ALTER COLUMN org_id DROP DEFAULT;
