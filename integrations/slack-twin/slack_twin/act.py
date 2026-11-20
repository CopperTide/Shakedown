"""`slack_act {person, action, args}`: what the world's people say in chat, called by links:
`post {channel, text, thread_ts?}` (a channel's id, name or `#name`) and `react {channel, ts, name}`."""
from shakedown_part.twin import Done

from . import chat, ids, records
from .errors import SlackError


def _post(ctx, person: str, args: dict) -> Done:
    ch = records.channel(ctx.store.conn, args.get("channel"), by_name=True)
    row = chat.post(ctx, person, ch, str(args.get("text", "")), args.get("thread_ts"))
    return Done({"channel": ch["id"], "ts": ids.ts(row["ts_us"])}, writes=[f"channel/{ch['name']}"])


def _react(ctx, person: str, args: dict) -> Done:
    ch = records.channel(ctx.store.conn, args.get("channel"), by_name=True)
    chat.react(ctx, person, ch, args.get("ts"), args.get("name", ""))
    return Done({"ok": True}, writes=[f"channel/{ch['name']}"])


ACTIONS = {"post": _post, "react": _react}


def run(ctx, person: str, action: str, args: dict) -> Done:
    fn = ACTIONS.get(action)
    if fn is None:
        return Done(error=f"unknown action {action}")
    person = person or ctx.config["default_user"]
    records.ensure_user(ctx.store.conn, ctx.config, person, ctx.now_ms())
    try:
        return fn(ctx, person, args)
    except SlackError as e:
        return Done(error=e.code)
