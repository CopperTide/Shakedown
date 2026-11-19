"""When each journey starts, in clock ms (world ms since the world's epoch): a Poisson process per journey whose
rate is the journey's `rpm` times the day's shape at that hour times any peak's factor. Each world minute draws
from its own `Random("{seed}:{minute}:{journey}")`, so a schedule doesn't depend on how time is split, and a
fork that resumes at a clock time gets the same traffic from there on."""
import math
import random

MINUTE_MS = 60_000
HOUR_MS = 3_600_000


def multiplier(day: dict, clock_ms: int) -> float:
    """The day's shape at `clock_ms`: `hourly[h]` at h:00 company time, linear in between; the run starts at
    `start_hour`."""
    hour = day["start_hour"] + clock_ms / HOUR_MS
    low = math.floor(hour)
    frac, hourly = hour - low, day["hourly"]
    a, b = hourly[low % 24], hourly[(low + 1) % 24]
    return a + frac * (b - a)


def peak_factor(peaks: list, clock_ms: int) -> float:
    factor = 1.0
    for p in peaks:
        if p["from_ms"] <= clock_ms < p["to_ms"]:
            factor *= p["factor"]
    return factor


def minute_arrivals(seed: int, minute: int, journey: str, per_minute: float) -> list[int]:
    if per_minute <= 0:
        return []
    rng, start = random.Random(f"{seed}:{minute}:{journey}"), minute * MINUTE_MS
    at, end, out = float(start), start + MINUTE_MS, []
    while True:
        at += rng.expovariate(per_minute / MINUTE_MS)
        if at >= end:
            return out
        out.append(int(at))


def plan(seed: int, from_ms: int, to_ms: int, profile: dict) -> list[tuple[int, str]]:
    """Every journey start in (from_ms, to_ms], in time order: `(clock ms, journey)`."""
    if to_ms <= from_ms:
        return []
    out = []
    for minute in range(from_ms // MINUTE_MS, to_ms // MINUTE_MS + 1):
        mid = minute * MINUTE_MS + MINUTE_MS // 2
        shape = multiplier(profile["day"], mid) * peak_factor(profile["peaks"], mid)
        for journey, spec in sorted(profile["journeys"].items()):
            out += [(t, journey) for t in minute_arrivals(seed, minute, journey, spec["rpm"] * shape)
                    if from_ms < t <= to_ms]
    return sorted(out)
