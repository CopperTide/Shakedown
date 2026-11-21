"""The Web API methods the twin serves (`/api/<method>`), each a read or a write. Every answer is
HTTP 200 with `{"ok": true, …}` or `{"ok": false, "error": "<code>"}`."""
from dataclasses import dataclass
from typing import Callable

from . import chat, ids, records, search, shapes
from .errors import SlackError

CHANNEL_TYPES = {"public_channel": 0, "private_channel": 1}


@dataclass(frozen=True)
class Method:
    fn: Callable
    write: bool = False
    scope: str = ""


def auth_test(call) -> dict:
    team = call.config.get("team", {})
    return {"url": f"{call.req.base}/", "team": team.get("name", "team"), "user": call.login,
            "team_id": shapes.team_id(call.config), "user_id": call.user["id"], "is_enterprise_install": False}


def _cursor_page(call, items: list, key: str) -> dict:
    start = int(call.arg("cursor", "0")) if str(call.arg("cursor", "0")).isdigit() else 0
    limit = call.int_arg("limit", 200, 1000)
    chunk = items[start:start + limit]
    more = start + limit < len(items)
    return {key: chunk, "response_metadata": {"next_cursor": str(start + limit) if more else ""}}


def users_list(call) -> dict:
    rows = call.db.execute("SELECT * FROM users ORDER BY name").fetchall()
    out = _cursor_page(call, [shapes.user(call.config, r) for r in rows], "members")
    return {**out, "cache_ts": call.ctx.now_ms() // 1000}


def users_info(call) -> dict:
    row = records.user_by_id(call.db, call.need("user", "user_not_found"))
    if row is None:
        raise SlackError("user_not_found")
    return {"user": shapes.user(call.config, row)}


def conversations_list(call) -> dict:
    wanted = {CHANNEL_TYPES[t] for t in str(call.arg("types", "public_channel")).split(",") if t in CHANNEL_TYPES}
    rows = call.db.execute("SELECT * FROM channels ORDER BY name").fetchall()
    return _cursor_page(call, [shapes.conversation(call.db, r) for r in rows if r["private"] in wanted], "channels")


def _channel(call):
    return records.channel(call.db, call.need("channel", "channel_not_found"))


def conversations_info(call) -> dict:
    return {"channel": shapes.conversation(call.db, _channel(call))}


def _bound(call, name: str, default: int) -> int:
    value = call.arg(name)
    us = ids.parse_ts(value) if value is not None else default
    if us is None:
        raise SlackError(f"invalid_ts_{name}")
    return us


def conversations_history(call) -> dict:
    ch = _channel(call)
    inclusive = str(call.arg("inclusive", "false")).lower() in ("1", "true")
    latest, oldest = _bound(call, "latest", 1 << 62), _bound(call, "oldest", 0)
    rows = call.db.execute(
        "SELECT * FROM messages WHERE channel_id = ? AND (thread_us IS NULL OR thread_us = ts_us) "
        f"AND ts_us {'<=' if inclusive else '<'} ? AND ts_us {'>=' if inclusive else '>'} ? ORDER BY ts_us DESC",
        (ch["id"], latest, oldest)).fetchall()
    limit = call.int_arg("limit", 100, 999)
    return {"messages": [shapes.message(call.db, call.config, r) for r in rows[:limit]], "has_more": len(rows) > limit,
            "pin_count": 0, "channel_actions_ts": None, "channel_actions_count": 0}


def conversations_replies(call) -> dict:
    ch = _channel(call)
    parent = records.message(call.db, ch, call.need("ts", "thread_not_found"), "thread_not_found")
    if parent["thread_us"] not in (None, parent["ts_us"]):
        parent = records.message(call.db, ch, ids.ts(parent["thread_us"]), "thread_not_found")
    rows = [parent, *records.replies(call.db, parent)]
    limit = call.int_arg("limit", 1000, 1000)
    msgs = [shapes.message(call.db, call.config, r, with_reactions=False) for r in rows[:limit]]
    return {"messages": msgs, "has_more": len(rows) > limit}


def post_message(call) -> dict:
    ch = records.channel(call.db, call.need("channel", "channel_not_found"), by_name=True)
    row = chat.post(call.ctx, call.login, ch, call.arg("text", ""), call.arg("thread_ts"))
    return {"channel": ch["id"], "ts": ids.ts(row["ts_us"]), "message": shapes.message(call.db, call.config, row)}


def update_message(call) -> dict:
    ch = records.channel(call.db, call.need("channel", "channel_not_found"), by_name=True)
    row = chat.update(call.ctx, call.login, ch, call.need("ts", "message_not_found"), call.arg("text", ""))
    return {"channel": ch["id"], "ts": ids.ts(row["ts_us"]), "text": row["text"], "message": {"text": row["text"]}}


def add_reaction(call) -> dict:
    try:
        ch = records.channel(call.db, call.need("channel", "message_not_found"))
    except SlackError:
        raise SlackError("message_not_found") from None   # Slack's word for an item in no channel it knows
    chat.react(call.ctx, call.login, ch, call.arg("timestamp"), call.arg("name", ""))
    return {}


def search_messages(call) -> dict:
    query = call.need("query", "no_query")
    count, page = call.int_arg("count", search.PAGE, 100), call.int_arg("page", 1, 100)
    return {"query": query, "messages": search.messages(call.db, call.config, call.req.base, query, count, page)}


METHODS = {
    "auth.test": Method(auth_test), "users.list": Method(users_list), "users.info": Method(users_info),
    "conversations.list": Method(conversations_list), "conversations.info": Method(conversations_info),
    "conversations.history": Method(conversations_history), "conversations.replies": Method(conversations_replies),
    "chat.postMessage": Method(post_message, True, "chat:write"), "chat.update": Method(update_message, True, "chat:write"),
    "reactions.add": Method(add_reaction, True, "reactions:write"), "search.messages": Method(search_messages),
}
READ_SCOPES = "channels:history,channels:read,groups:history,groups:read,search:read,users:read"
