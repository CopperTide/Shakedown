"""A new instance from its config: users, schedules (a rotation with a shift length, from a start
relative to world now) and services (each with its integration key and schedule). No notices."""
import json

from . import ids, records, times


def seed(ctx, config: dict) -> None:
    db, now = ctx.store.conn, ctx.now_ms()
    for u in config.get("users", []):
        records.ensure_user(db, config, u["name"], now, u.get("display_name"), u.get("email"))
    records.ensure_user(db, config, config["default_user"], now)
    for s in config.get("schedules", []):
        for name in s.get("rotation", []):
            records.ensure_user(db, config, name, now)
        db.execute("INSERT INTO schedules(id, name, rotation, shift_ms, start_ms) VALUES (?, ?, ?, ?, ?)",
                   (ids.make("schedule", s["name"]), s["name"], json.dumps(s.get("rotation", [])),
                    int(s.get("shift_hours", 168) * 3_600_000), times.relative(s.get("start"), now)))
    for s in config.get("services", []):
        schedule = ids.make("schedule", s["schedule"]) if s.get("schedule") else None
        db.execute("INSERT INTO services(id, name, integration_key, schedule_id, policy_id, created_ms) "
                   "VALUES (?, ?, ?, ?, ?, ?)", (ids.make("service", s["name"]), s["name"], s["integration_key"],
                                                 schedule, ids.make("policy", s["name"]), now))
