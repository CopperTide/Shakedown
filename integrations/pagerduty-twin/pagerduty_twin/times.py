"""PagerDuty's times: ISO 8601 in UTC (`2026-03-10T06:00:00Z`), and relative config times (`-1h`)."""
import datetime

UTC = datetime.timezone.utc
UNIT_MS = {"m": 60_000, "h": 3_600_000, "d": 86_400_000}


def iso(ms: int | None) -> str | None:
    if ms is None:
        return None
    return datetime.datetime.fromtimestamp(ms // 1000, UTC).strftime("%Y-%m-%dT%H:%M:%SZ")


def parse(text: str) -> int | None:
    try:
        t = datetime.datetime.fromisoformat(str(text).replace("Z", "+00:00"))
    except ValueError:
        return None
    return int((t if t.tzinfo else t.replace(tzinfo=UTC)).timestamp() * 1000)


def relative(value, now_ms: int) -> int:
    text = str(value or "0m")
    sign = -1 if text.startswith("-") else 1
    return now_ms + sign * int(text.lstrip("+-")[:-1]) * UNIT_MS[text[-1]]
