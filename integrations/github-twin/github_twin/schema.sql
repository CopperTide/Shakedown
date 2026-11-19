CREATE TABLE IF NOT EXISTS users (
  id INTEGER PRIMARY KEY, login TEXT NOT NULL UNIQUE, type TEXT NOT NULL, created_ms INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS repos (
  id INTEGER PRIMARY KEY, owner TEXT NOT NULL, name TEXT NOT NULL, description TEXT, private INTEGER NOT NULL,
  default_branch TEXT NOT NULL, created_ms INTEGER NOT NULL, updated_ms INTEGER NOT NULL,
  pushed_ms INTEGER NOT NULL, UNIQUE (owner, name));
CREATE TABLE IF NOT EXISTS branch_stats (
  repo_id INTEGER NOT NULL REFERENCES repos(id), branch TEXT NOT NULL, forced_pushes INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (repo_id, branch));
