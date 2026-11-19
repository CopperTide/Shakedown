"""Reading the store: users, channels (by id, or by name where Slack takes one) and messages."""
from . import ids
from .errors import SlackError


def user_by_name(db, name: str):
    return db.execute("SELECT * FROM users WHERE name = ?", (name,)).fetchone()


def user_by_id(db, user_id: str):
    return db.execute("SELECT * FROM users WHERE id = ?", (user_id,)).fetchone()


def ensure_user(db, config: dict, name: str, now_ms: int, real_name: str | None = None):
    """The user called `name`, made now when it is not there yet."""
    row = user_by_name(db, name)
    if row is None:
        email = f"{name}@{config.get('team', {}).get('email_domain', 'example.test')}"
        db.execute("INSERT INTO users(id, name, real_name, email, created_ms) VALUES (?, ?, ?, ?, ?)",
                   (ids.make("U", name), name, real_name or name, email, now_ms))
        row = user_by_name(db, name)
    return row


def channel(db, ref, by_name: bool = False):
    """The channel `ref` names: an id, or (when `by_name`, as chat methods allow) `name` or `#name`."""
    ref = str(ref or "")
    row = db.execute("SELECT * FROM channels WHERE id = ?", (ref,)).fetchone()
    if row is None and by_name:
        row = db.execute("SELECT * FROM channels WHERE name = ?", (ref.lstrip("#"),)).fetchone()
    if row is None:
        raise SlackError("channel_not_found")
    return row


def message(db, channel_row, ts_text, code: str = "message_not_found"):
    us = ids.parse_ts(ts_text)
    row = db.execute("SELECT * FROM messages WHERE channel_id = ? AND ts_us = ?", (channel_row["id"], us)).fetchone()
    if row is None:
        raise SlackError(code)
    return row


def replies(db, parent) -> list:
    return db.execute("SELECT * FROM messages WHERE thread_us = ? AND channel_id = ? AND ts_us != ? ORDER BY ts_us",
                      (parent["ts_us"], parent["channel_id"], parent["ts_us"])).fetchall()


def reactions(db, message_id: int) -> list:
    return db.execute("SELECT name, user FROM reactions WHERE message_id = ? ORDER BY at, rowid", (message_id,)).fetchall()


def name_of(db, user_id: str) -> str:
    row = user_by_id(db, user_id)
    return row["name"] if row else user_id
