CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    account_id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    email TEXT NOT NULL,
    created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS projects (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    lead TEXT,
    next_num INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS sprints (
    id INTEGER PRIMARY KEY,
    board_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    state TEXT NOT NULL,
    start_ms INTEGER,
    end_ms INTEGER,
    complete_ms INTEGER,
    created_ms INTEGER NOT NULL,
    goal TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS boards (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    project_id INTEGER NOT NULL REFERENCES projects(id)
);
CREATE TABLE IF NOT EXISTS issues (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    summary TEXT NOT NULL,
    description TEXT,
    type TEXT NOT NULL,
    status TEXT NOT NULL,
    priority TEXT NOT NULL,
    assignee TEXT,
    reporter TEXT,
    creator TEXT,
    resolution TEXT,
    sprint_id INTEGER REFERENCES sprints(id),
    parent_key TEXT,
    duedate TEXT,
    rank INTEGER NOT NULL,
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL,
    resolved_ms INTEGER
);
CREATE TABLE IF NOT EXISTS labels (
    issue_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    PRIMARY KEY (issue_id, label)
);
CREATE TABLE IF NOT EXISTS comments (
    id INTEGER PRIMARY KEY,
    issue_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    author TEXT NOT NULL,
    update_author TEXT NOT NULL,
    body TEXT NOT NULL,
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS links (
    id INTEGER PRIMARY KEY,
    type TEXT NOT NULL,
    inward_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    outward_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE
);
