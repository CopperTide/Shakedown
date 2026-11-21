"""`slack_app {app, kind, data}` (a world tool, called by links): an app of the config, as the GitHub or PagerDuty
app for Slack, posts the message it has for `kind` into its channel, `{/pointer}`s filled from `data`; a kind it
has no message for posts nothing."""
import re

from shakedown_part.twin import Done

from . import chat, ids, records
from .errors import SlackError

_POINTER = re.compile(r"\{(/[^{}]*)\}")

TOOL = {"name": "slack_app", "effect": "write",
        "description": "An app posts what another system told the world into its channel (called by links).",
        "inputSchema": {"type": "object", "required": ["app", "kind"], "properties": {
            "app": {"type": "string"}, "kind": {"type": "string"}, "data": {"type": "object"}}}}


def _at(data, pointer: str):
    for k in pointer.split("/")[1:]:
        k = k.replace("~1", "/").replace("~0", "~")
        if isinstance(data, dict) and k in data:
            data = data[k]
        elif isinstance(data, list) and k.isdigit() and int(k) < len(data):
            data = data[int(k)]
        else:
            return ""
    return "" if data is None else str(data)


def text_of(message: str, data: dict) -> str:
    return _POINTER.sub(lambda m: _at(data, m.group(1)), message)


def run(ctx, args: dict) -> Done:
    app = next((a for a in ctx.config.get("apps", []) if a["name"] == args.get("app")), None)
    if app is None:
        return Done(error=f"no app {args.get('app')}")
    message = app.get("messages", {}).get(args.get("kind"))
    if message is None:
        return Done({"posted": False})
    records.ensure_user(ctx.store.conn, ctx.config, app["user"], ctx.now_ms())
    try:
        ch = records.channel(ctx.store.conn, app["channel"], by_name=True)
        row = chat.post(ctx, app["user"], ch, text_of(message, args.get("data") or {}), None)
    except SlackError as e:
        return Done(error=e.code)
    return Done({"posted": True, "channel": ch["id"], "ts": ids.ts(row["ts_us"])}, writes=[f"channel/{ch['name']}"])
