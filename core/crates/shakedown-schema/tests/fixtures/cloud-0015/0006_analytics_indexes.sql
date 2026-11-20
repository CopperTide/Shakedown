-- The dashboard looks events up by name over time, and per person by name.
CREATE INDEX analytics_events_name_at ON analytics_events(name, at);
CREATE INDEX analytics_events_user_name ON analytics_events(user_id, name);
