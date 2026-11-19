"""Timestamps as GitHub writes them: ISO-8601 in UTC, whole seconds, `Z`."""
from datetime import datetime, timezone


def iso(ms: int) -> str:
    return datetime.fromtimestamp(ms // 1000, tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
