"""The writes, shared by the Web API and `slack_act`: messages posted (top level or in a thread),
edited by their author, and reacted to; mentions (`<@U…>`, or `@name` of a known user) become notices."""
import re

from . import ids, records
from .errors import SlackError

LINKED = re.compile(r"<@([UW][A-Z0-9]+)(?:\|[^>]*)?>")
BARE = re.compile(r"(?<![\w<])@([a-z0-9][a-z0-9._-]*)", re.IGNORECASE)
MAX_TEXT = 40_000


def _mentioned(db, text: str, author: str) -> list[str]:
    """Who `text` names, in the order it names them, the author left out."""
    named = [(m.start(), records.name_of(db, m.group(1))) for m in LINKED.finditer(text)]
    named += [(m.start(), m.group(1)) for m in BARE.finditer(text) if records.user_by_name(db, m.group(1))]
    out = []
    for _, name in sorted(named):
        if name != author and name not in out:
            out.append(name)
    return out


def post(ctx, author: str, channel_row, text: str, thread_ts=None):
    if not text:
        raise SlackError("no_text")
    if len(text) > MAX_TEXT:
        raise SlackError("msg_too_long")
    db = ctx.store.conn
    thread_us = records.message(db, channel_row, thread_ts, "thread_not_found")["ts_us"] if thread_ts else None
    us = ids.next_us(db, ctx.now_ms())
    db.execute("INSERT INTO messages(channel_id, ts_us, user, text, thread_us) VALUES (?, ?, ?, ?, ?)",
               (channel_row["id"], us, records.user_by_name(db, author)["id"], text, thread_us))
    if thread_us is not None:
        db.execute("UPDATE messages SET thread_us = ts_us WHERE ts_us = ?", (thread_us,))
    for name in _mentioned(db, text, author):
        ctx.notify("mentioned", f"slack mention of {name} in #{channel_row['name']}",
                   {"channel": channel_row["name"], "user": name, "text": text, "ts": ids.ts(us)})
    return db.execute("SELECT * FROM messages WHERE ts_us = ?", (us,)).fetchone()


def update(ctx, author: str, channel_row, ts_text, text: str):
    db = ctx.store.conn
    row = records.message(db, channel_row, ts_text)
    if row["user"] != records.user_by_name(db, author)["id"]:
        raise SlackError("cant_update_message")
    if not text:
        raise SlackError("no_text")
    db.execute("UPDATE messages SET text = ? WHERE id = ?", (text, row["id"]))
    return db.execute("SELECT * FROM messages WHERE id = ?", (row["id"],)).fetchone()


def react(ctx, author: str, channel_row, ts_text, name: str) -> None:
    db = ctx.store.conn
    if not ts_text:
        raise SlackError("no_item_specified")
    row = records.message(db, channel_row, ts_text)
    name = str(name or "").strip(":")
    if not re.fullmatch(r"[a-z0-9_+'-]+", name):
        raise SlackError("invalid_name")
    user_id = records.user_by_name(db, author)["id"]
    if db.execute("SELECT 1 FROM reactions WHERE message_id = ? AND name = ? AND user = ?",
                  (row["id"], name, user_id)).fetchone():
        raise SlackError("already_reacted")
    db.execute("INSERT INTO reactions(message_id, name, user, at) VALUES (?, ?, ?, ?)",
               (row["id"], name, user_id, ids.next_us(db, ctx.now_ms())))
