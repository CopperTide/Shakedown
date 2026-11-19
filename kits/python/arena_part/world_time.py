"""World time for what a part emits (metrics, logs, traces): the world's epoch plus its clock, so
telemetry follows the world clock and stands still while the branch is paused. arenad gives a part
`world_time {epoch_ms, speed}` at `start` (docs/protocol.md, "World time")."""
import datetime
import time

#: How far past the last clock it saw a part's world time runs on (world ms).
MAX_LEAD_MS = 1_000


class WorldTime:
    """Stamps from the clock a part is given, running on at `speed` between clocks (at most
    `MAX_LEAD_MS` past the last one) and never going back."""

    def __init__(self, epoch_ms, speed):
        self.epoch_ms, self.speed = int(epoch_ms), float(speed)
        self._seen = None  # (clock_ms, monotonic seconds)
        self._given = None

    def at(self, clock_ms):
        """The unix ms of branch clock `clock_ms`."""
        return self.epoch_ms + int(clock_ms)

    def observe(self, clock_ms, now=None):
        """Notes the branch clock `clock_ms` (from `advance` or `call_tool`) at `now`."""
        self._seen = (int(clock_ms), time.monotonic() if now is None else now)

    def now_ms(self, now=None):
        """The world's unix ms now: the last clock seen plus the time since at `speed`, at most
        `MAX_LEAD_MS` ahead of it, never less than what it gave before."""
        now = time.monotonic() if now is None else now
        clock = 0
        if self._seen is not None:
            seen, at = self._seen
            clock = seen + min(int(max(now - at, 0) * 1000 * self.speed), MAX_LEAD_MS)
        value = self.at(clock)
        self._given = value if self._given is None else max(self._given, value)
        return self._given


def unix_nanos(ms):
    """`ms` (unix) as nanoseconds in decimal: Loki push and OTLP `timeUnixNano`."""
    return str(int(ms) * 1_000_000)


def rfc3339(ms):
    """`ms` (unix) as RFC 3339 in UTC with milliseconds (`2026-03-10T06:02:30.000Z`)."""
    t = datetime.datetime(1970, 1, 1, tzinfo=datetime.timezone.utc) + datetime.timedelta(milliseconds=int(ms))
    return t.strftime("%Y-%m-%dT%H:%M:%S.") + f"{t.microsecond // 1000:03d}Z"
