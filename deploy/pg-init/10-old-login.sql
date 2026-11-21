-- The login packages and shaked used before 2026-11-19, so a checkout of either age connects
-- to a new dev server without SHAKEDOWN_PG_URL. Runs once, when the data volume is made.
CREATE ROLE arena WITH LOGIN SUPERUSER PASSWORD 'arena';
