"""Jira's time stamps (`2026-03-10T06:00:00.000+0000`), and the times a config may give (relative to
world now as `-7d`, `2w`, `-3h`, `30m`, or ISO 8601)."""
import datetime
import re

UTC = datetime.timezone.utc
UNIT_MS = {"m": 60_000, "h": 3_600_000, "d": 86_400_000, "w": 604_800_000}
RELATIVE = re.compile(r"([+-]?)(\d+)([mhdw])")


def stamp(ms: int | None) -> str | None:
    if ms is None:
        return None
    t = datetime.datetime.fromtimestamp(ms / 1000, UTC)
    return t.strftime("%Y-%m-%dT%H:%M:%S.") + f"{t.microsecond // 1000:03d}+0000"


def relative_ms(text: str) -> int | None:
    """`-7d` as signed ms; None when `text` is not a relative time."""
    m = RELATIVE.fullmatch(text.strip())
    if m is None:
        return None
    return (-1 if m.group(1) == "-" else 1) * int(m.group(2)) * UNIT_MS[m.group(3)]


def parse_iso(text: str) -> int | None:
    """An ISO 8601 date or time (UTC when no zone is given) as unix ms; None when it is not one."""
    t = text.strip().replace("Z", "+00:00")
    t = re.sub(r"([+-]\d\d)(\d\d)$", r"\1:\2", t)
    try:
        when = datetime.datetime.fromisoformat(t)
    except ValueError:
        return None
    if when.tzinfo is None:
        when = when.replace(tzinfo=UTC)
    return int(when.timestamp() * 1000)


def at(value, now_ms: int) -> int | None:
    """A config or request time: relative to now, ISO, or unix ms."""
    if value is None or isinstance(value, int):
        return value
    rel = relative_ms(str(value))
    return now_ms + rel if rel is not None else parse_iso(str(value))


def day(ms: int) -> str:
    return datetime.datetime.fromtimestamp(ms / 1000, UTC).strftime("%Y-%m-%d")
