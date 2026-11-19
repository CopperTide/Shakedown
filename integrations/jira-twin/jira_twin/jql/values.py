"""What a JQL value means: names resolved to what the store keeps (or Jira's "does not exist"
error), functions (`currentUser()`, `openSprints()`, `startOfDay(-1)`, …) and dates (relative to world
now, or ISO)."""
import datetime

from .. import times
from .errors import JqlError

UTC = datetime.timezone.utc
SPRINT_FUNCS = {"opensprints": "active", "closedsprints": "closed", "futuresprints": "future"}
DATE_FUNCS = ("now", "startofday", "endofday", "startofweek", "endofweek", "startofmonth", "endofmonth")


def missing(value, field: str) -> JqlError:
    return JqlError(f"The value '{value}' does not exist for the field '{field}'.")


def no_function(name: str) -> JqlError:
    return JqlError(f"Unable to find JQL function '{name}()'.")


def _offset_days(args: tuple) -> int:
    if not args:
        return 0
    rel = times.relative_ms(args[0] if args[0][-1:].isalpha() else f"{args[0]}d")
    if rel is None:
        raise JqlError(f"The argument '{args[0]}' of a date function is not a valid offset.")
    return rel // 86_400_000


def _period(name: str, now_ms: int, days: int) -> int:
    t = datetime.datetime.fromtimestamp(now_ms / 1000, UTC) + datetime.timedelta(days=days)
    start = t.replace(hour=0, minute=0, second=0, microsecond=0)
    if "week" in name:
        start -= datetime.timedelta(days=start.weekday() + 1 if start.weekday() != 6 else 0)
        length = datetime.timedelta(days=7)
    elif "month" in name:
        start = start.replace(day=1)
        length = (start.replace(day=28) + datetime.timedelta(days=4)).replace(day=1) - start
    else:
        length = datetime.timedelta(days=1)
    end = start + length - datetime.timedelta(milliseconds=1)
    return int((start if name.startswith("start") else end).timestamp() * 1000)


def date_func(name: str, args: tuple, now_ms: int) -> int:
    low = name.lower()
    if low == "now":
        return now_ms
    if low not in DATE_FUNCS:
        raise no_function(name)
    return _period(low, now_ms, _offset_days(args))


def date(value, now_ms: int, field: str) -> int:
    from .parser import Func
    if isinstance(value, Func):
        return date_func(value.name, value.args, now_ms)
    rel = times.relative_ms(str(value))
    if rel is not None:
        return now_ms + rel
    found = times.parse_iso(str(value).replace("/", "-"))
    if found is None:
        raise JqlError(f"Date value '{value}' for field '{field}' is invalid. Valid formats include: "
                       "'yyyy/MM/dd HH:mm', 'yyyy-MM-dd HH:mm', 'yyyy/MM/dd', 'yyyy-MM-dd', or a period format "
                       "e.g. '-5d', '4w 2d'.")
    return found
