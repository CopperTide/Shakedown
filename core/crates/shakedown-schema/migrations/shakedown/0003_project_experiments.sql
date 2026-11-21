-- shakedown 3: an experiment deployed from a project folder remembers which experiment file it came from, so the
-- file's page can list its deployments. Personal use only: the column holds an item id and names no owner.
ALTER TABLE shakedown.experiments
  ADD COLUMN project_experiment text;
CREATE INDEX experiments_project_experiment_idx ON shakedown.experiments (project_experiment, created_at DESC);
COMMENT ON COLUMN shakedown.experiments.project_experiment IS 'The id of the project experiment file it was deployed from, if any.';
