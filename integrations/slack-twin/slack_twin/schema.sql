CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    real_name TEXT NOT NULL,
    email TEXT NOT NULL,
    created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS channels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    private INTEGER NOT NULL DEFAULT 0,
    general INTEGER NOT NULL DEFAULT 0,
    topic TEXT NOT NULL DEFAULT '',
    purpose TEXT NOT NULL DEFAULT '',
    creator TEXT NOT NULL,
    created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    channel_id TEXT NOT NULL REFERENCES channels(id),
    ts_us INTEGER NOT NULL UNIQUE,
    user TEXT NOT NULL,
    text TEXT NOT NULL,
    thread_us INTEGER
);
CREATE TABLE IF NOT EXISTS reactions (
    message_id INTEGER NOT NULL REFERENCES messages(id),
    name TEXT NOT NULL,
    user TEXT NOT NULL,
    at INTEGER NOT NULL,
    PRIMARY KEY (message_id, name, user)
);
