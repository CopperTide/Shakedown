"""The docs `inspect` reads: each channel's messages (oldest first, replies in place, users by name)."""
from . import ids, records


def _message(db, row) -> dict:
    out = {"user": records.name_of(db, row["user"]), "text": row["text"], "ts": ids.ts(row["ts_us"])}
    if row["thread_us"] is not None and row["thread_us"] != row["ts_us"]:
        out["thread_ts"] = ids.ts(row["thread_us"])
    return out


def build(ctx) -> dict:
    db, out = ctx.store.conn, {}
    for ch in db.execute("SELECT * FROM channels ORDER BY name").fetchall():
        rows = db.execute("SELECT * FROM messages WHERE channel_id = ? ORDER BY ts_us", (ch["id"],)).fetchall()
        msgs = [_message(db, r) for r in rows]
        out[f"channel/{ch['name']}"] = {"messages": msgs, "last_ts": msgs[-1]["ts"] if msgs else None}
    return out
