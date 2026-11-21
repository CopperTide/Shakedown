"""Who is on call: each schedule is one rotation of users with a fixed shift length from its start,
in world time; a service pages its schedule's on-call user (escalation level 1)."""
import json
from dataclasses import dataclass

from . import records


@dataclass(frozen=True)
class Shift:
    user: str
    start_ms: int
    end_ms: int


def shift(schedule, at_ms: int) -> Shift | None:
    rotation = json.loads(schedule["rotation"])
    if not rotation:
        return None
    n = (at_ms - schedule["start_ms"]) // schedule["shift_ms"]
    start = schedule["start_ms"] + n * schedule["shift_ms"]
    return Shift(rotation[n % len(rotation)], start, start + schedule["shift_ms"])


def for_service(db, service, at_ms: int) -> Shift | None:
    schedule = records.schedule(db, service["schedule_id"]) if service["schedule_id"] else None
    return shift(schedule, at_ms) if schedule else None
