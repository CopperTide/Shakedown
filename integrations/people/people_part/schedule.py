"""When people act, in world ms of the clock: a seeded delay after what set them off, and the slots of rules on a
schedule (daily at times of the world's day in UTC, every so many minutes, or once). Every draw uses
`Random.random()` alone, whose sequence Python keeps across versions."""
from random import Random

DAY_MS = 86_400_000
MIN_MS = 60_000


def rng(seed, person_id: str, rule_id: str, n: int, purpose: str = "") -> Random:
    """The generator of one person's `n`th trigger of a rule (`purpose` names a second stream of it)."""
    return Random(f"{seed}:{person_id}:{rule_id}:{n}" + (f":{purpose}" if purpose else ""))


def draw(r: Random, n: int) -> int:
    """An index in 0..n-1."""
    return min(int(r.random() * n), n - 1)


def due(seed, person: dict, rule, at_ms: int, n: int) -> int:
    """When `person` acts on their `n`th trigger of `rule` at `at_ms`: `rule.after` drawn in whole seconds."""
    lo, hi = rule.after
    if hi == lo:
        return at_ms + lo
    steps = (hi - lo) // 1000 + 1
    return at_ms + min(lo + draw(rng(seed, person["id"], rule.id, n), steps) * 1000, hi)


def first_slot(rule, epoch_ms: int, clock_ms: int) -> int | None:
    """The rule's first slot at or after `clock_ms`, or None (a rule that answers notices, or a `once` passed)."""
    if rule.once is not None:
        return rule.once if rule.once >= clock_ms else None
    if rule.every is None:
        return None
    if rule.every.minutes:
        return clock_ms + rule.every.minutes * MIN_MS
    return _next_time_of_day(rule.every.at, epoch_ms, clock_ms)


def next_slot(rule, epoch_ms: int, slot_ms: int) -> int | None:
    """The slot after `slot_ms`."""
    if rule.every is None:
        return None
    if rule.every.minutes:
        return slot_ms + rule.every.minutes * MIN_MS
    return _next_time_of_day(rule.every.at, epoch_ms, slot_ms + 1)


def _next_time_of_day(minutes: tuple, epoch_ms: int, clock_ms: int) -> int:
    world = epoch_ms + clock_ms
    day = world - world % DAY_MS
    return min(t for d in (0, 1) for m in minutes if (t := day + d * DAY_MS + m * MIN_MS) >= world) - epoch_ms


def day_of(epoch_ms: int, clock_ms: int) -> int:
    """The world's day (UTC) at `clock_ms`, as days since 1970."""
    return (epoch_ms + clock_ms) // DAY_MS
