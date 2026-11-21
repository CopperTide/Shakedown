"""`pagerduty_act {person, action, args}`: what the world's people do on the pager, called by links:
`ack {incident}`, `resolve {incident}`, `note {incident, text}`."""
from shakedown_part.twin import Done

from . import incidents, records
from .errors import PagerDutyError


def _change(status: str):
    def run(ctx, person: str, args: dict) -> Done:
        row = incidents.change(ctx, records.incident(ctx.store.conn, args.get("incident", "")), status, f"user:{person}")
        return Done({"incident": args["incident"], "status": row["status"]}, writes=[f"incident/{args['incident']}"])

    return run


def _note(ctx, person: str, args: dict) -> Done:
    row = records.incident(ctx.store.conn, args.get("incident", ""))
    incidents.note(ctx, row, person, str(args.get("text", "")))
    return Done({"incident": args["incident"]}, writes=[f"incident/{args['incident']}"])


ACTIONS = {"ack": _change("acknowledged"), "resolve": _change("resolved"), "note": _note}


def run(ctx, person: str, action: str, args: dict) -> Done:
    fn = ACTIONS.get(action)
    if fn is None:
        return Done(error=f"unknown action {action}")
    person = person or ctx.config["default_user"]
    records.ensure_user(ctx.store.conn, ctx.config, person, ctx.now_ms())
    try:
        return fn(ctx, person, args)
    except PagerDutyError as e:
        return Done(error=e.message)
