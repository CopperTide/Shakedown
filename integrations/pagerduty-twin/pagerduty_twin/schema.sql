CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS schedules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    rotation TEXT NOT NULL,
    shift_ms INTEGER NOT NULL,
    start_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS services (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    integration_key TEXT NOT NULL UNIQUE,
    schedule_id TEXT REFERENCES schedules(id),
    policy_id TEXT NOT NULL,
    created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS incidents (
    number INTEGER PRIMARY KEY,
    service_id TEXT NOT NULL REFERENCES services(id),
    title TEXT NOT NULL,
    dedup_key TEXT NOT NULL,
    severity TEXT NOT NULL,
    status TEXT NOT NULL,
    assignee TEXT,
    alerts INTEGER NOT NULL DEFAULT 1,
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL,
    changed_ms INTEGER NOT NULL,
    changed_by TEXT,
    acked_by TEXT,
    acked_ms INTEGER,
    resolved_ms INTEGER
);
CREATE TABLE IF NOT EXISTS notes (
    id INTEGER PRIMARY KEY,
    incident INTEGER NOT NULL REFERENCES incidents(number),
    user TEXT NOT NULL,
    content TEXT NOT NULL,
    created_ms INTEGER NOT NULL
);
