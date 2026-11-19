CREATE TABLE IF NOT EXISTS users (
  id INTEGER PRIMARY KEY, login TEXT NOT NULL UNIQUE, type TEXT NOT NULL, created_ms INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS repos (
  id INTEGER PRIMARY KEY, owner TEXT NOT NULL, name TEXT NOT NULL, description TEXT, private INTEGER NOT NULL,
  default_branch TEXT NOT NULL, created_ms INTEGER NOT NULL, updated_ms INTEGER NOT NULL,
  pushed_ms INTEGER NOT NULL, UNIQUE (owner, name));
CREATE TABLE IF NOT EXISTS branch_stats (
  repo_id INTEGER NOT NULL REFERENCES repos(id), branch TEXT NOT NULL, forced_pushes INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (repo_id, branch));
CREATE TABLE IF NOT EXISTS issues (
  id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL REFERENCES repos(id), number INTEGER NOT NULL,
  title TEXT NOT NULL, body TEXT, state TEXT NOT NULL DEFAULT 'open', state_reason TEXT, user TEXT NOT NULL,
  locked INTEGER NOT NULL DEFAULT 0, is_pull INTEGER NOT NULL DEFAULT 0, created_ms INTEGER NOT NULL,
  updated_ms INTEGER NOT NULL, closed_ms INTEGER, closed_by TEXT, UNIQUE (repo_id, number));
CREATE TABLE IF NOT EXISTS pulls (
  issue_id INTEGER PRIMARY KEY REFERENCES issues(id), head_ref TEXT NOT NULL, head_sha TEXT NOT NULL,
  base_ref TEXT NOT NULL, base_sha TEXT NOT NULL, draft INTEGER NOT NULL DEFAULT 0,
  maintainer_can_modify INTEGER NOT NULL DEFAULT 1, merged INTEGER NOT NULL DEFAULT 0, merged_ms INTEGER,
  merged_by TEXT, merge_commit_sha TEXT, reviewed_at_merge INTEGER NOT NULL DEFAULT 0,
  approvals_at_merge TEXT NOT NULL DEFAULT '[]');
CREATE TABLE IF NOT EXISTS assignees (
  issue_id INTEGER NOT NULL REFERENCES issues(id), login TEXT NOT NULL, PRIMARY KEY (issue_id, login));
CREATE TABLE IF NOT EXISTS labels (
  id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL REFERENCES repos(id), name TEXT NOT NULL,
  color TEXT NOT NULL DEFAULT 'ededed', description TEXT, UNIQUE (repo_id, name));
CREATE TABLE IF NOT EXISTS issue_labels (
  issue_id INTEGER NOT NULL REFERENCES issues(id), label_id INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY (issue_id, label_id));
CREATE TABLE IF NOT EXISTS comments (
  id INTEGER PRIMARY KEY, issue_id INTEGER NOT NULL REFERENCES issues(id), user TEXT NOT NULL, body TEXT NOT NULL,
  created_ms INTEGER NOT NULL, updated_ms INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS reviews (
  id INTEGER PRIMARY KEY, issue_id INTEGER NOT NULL REFERENCES issues(id), user TEXT NOT NULL, body TEXT NOT NULL,
  state TEXT NOT NULL, commit_id TEXT, submitted_ms INTEGER);
CREATE TABLE IF NOT EXISTS review_comments (
  id INTEGER PRIMARY KEY, issue_id INTEGER NOT NULL REFERENCES issues(id), review_id INTEGER REFERENCES reviews(id),
  user TEXT NOT NULL, body TEXT NOT NULL, path TEXT NOT NULL, line INTEGER, side TEXT, commit_id TEXT NOT NULL,
  diff_hunk TEXT NOT NULL, in_reply_to INTEGER, created_ms INTEGER NOT NULL, updated_ms INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS review_requests (
  issue_id INTEGER NOT NULL REFERENCES issues(id), login TEXT NOT NULL, PRIMARY KEY (issue_id, login));
CREATE TABLE IF NOT EXISTS statuses (
  id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL REFERENCES repos(id), sha TEXT NOT NULL, state TEXT NOT NULL,
  context TEXT NOT NULL, description TEXT, target_url TEXT, creator TEXT NOT NULL, created_ms INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS check_suites (
  id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL REFERENCES repos(id), head_sha TEXT NOT NULL,
  created_ms INTEGER NOT NULL, UNIQUE (repo_id, head_sha));
CREATE TABLE IF NOT EXISTS check_runs (
  id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL REFERENCES repos(id), suite_id INTEGER NOT NULL REFERENCES check_suites(id),
  head_sha TEXT NOT NULL, name TEXT NOT NULL, status TEXT NOT NULL, conclusion TEXT, started_ms INTEGER,
  completed_ms INTEGER, external_id TEXT, details_url TEXT, title TEXT, summary TEXT, text TEXT);
CREATE TABLE IF NOT EXISTS hooks (
  id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL REFERENCES repos(id), name TEXT NOT NULL, active INTEGER NOT NULL,
  events TEXT NOT NULL, config TEXT NOT NULL, created_ms INTEGER NOT NULL, updated_ms INTEGER NOT NULL,
  last_code INTEGER, last_status TEXT, last_message TEXT);
CREATE TABLE IF NOT EXISTS deliveries (
  id INTEGER PRIMARY KEY, hook_id INTEGER NOT NULL, event TEXT NOT NULL, guid TEXT NOT NULL, payload TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending', status_code INTEGER, error TEXT, created_ms INTEGER NOT NULL);
