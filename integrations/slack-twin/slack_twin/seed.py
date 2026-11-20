"""A new instance from its config: users, channels and their earlier messages (times relative to
world now, such as `-2h`). Seeding reports no notices."""
from shakedown_part.twin import TwinCtx

from . import chat, ids, records

UNIT_MS = {"m": 60_000, "h": 3_600_000, "d": 86_400_000}


def _at(value, now: int) -> int:
    text = str(value or "0m")
    sign = -1 if text.startswith("-") else 1
    return now + sign * int(text.lstrip("+-")[:-1]) * UNIT_MS[text[-1]]


def _channels(db, config: dict, now: int) -> None:
    creator = records.user_by_name(db, config["default_user"])["id"]
    for c in config.get("channels", []):
        db.execute("INSERT INTO channels(id, name, private, general, topic, purpose, creator, created_ms) "
                   "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                   (ids.make("C", c["name"]), c["name"], int(bool(c.get("private"))), int(bool(c.get("general"))),
                    c.get("topic", ""), c.get("purpose", ""), creator, now))


class _At:
    """The instance's context with world now fixed at a seed message's time."""

    def __init__(self, ctx: TwinCtx, ms: int):
        self.store, self.config, self.notify, self._ms = ctx.store, ctx.config, ctx.notify, ms

    def now_ms(self) -> int:
        return self._ms


def seed(ctx: TwinCtx, config: dict) -> None:
    db, now = ctx.store.conn, ctx.now_ms()
    for u in config.get("users", []):
        records.ensure_user(db, config, u["name"], now, u.get("real_name"))
    records.ensure_user(db, config, config["default_user"], now)
    _channels(db, config, now)
    for m in sorted(config.get("messages", []), key=lambda m: _at(m.get("at"), now)):
        records.ensure_user(db, config, m["user"], now)
        ch = records.channel(db, m["channel"], by_name=True)
        chat.post(_At(ctx, _at(m.get("at"), now)), m["user"], ch, m["text"], m.get("thread_ts"))
    del ctx.notices[:]
