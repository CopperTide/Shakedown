-- What people do in the beta (never content): events from the gateway, the browser and each user's
-- Sculptured, checked against an allowlist before they are stored. Raw rows are kept 180 days, then
-- folded into daily totals.
CREATE TABLE analytics_events (
  id bigserial PRIMARY KEY,
  at timestamptz NOT NULL DEFAULT now(),
  user_id uuid REFERENCES users(id) ON DELETE CASCADE,
  source text NOT NULL CHECK (source IN ('gateway','browser','sculptured')),
  name text NOT NULL,
  props jsonb NOT NULL DEFAULT '{}'
);
CREATE INDEX analytics_events_at ON analytics_events(at);
CREATE INDEX analytics_events_user_at ON analytics_events(user_id, at);
CREATE TABLE analytics_daily (
  day date NOT NULL,
  user_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
  name text NOT NULL,
  count bigint NOT NULL,
  seconds bigint NOT NULL DEFAULT 0,
  PRIMARY KEY (day, user_id, name)
);
