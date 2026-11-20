-- false: a Community-only account (profile, saves, publishing); no workspace, computers or /api/*.
ALTER TABLE users ADD COLUMN workspace boolean NOT NULL DEFAULT true;
