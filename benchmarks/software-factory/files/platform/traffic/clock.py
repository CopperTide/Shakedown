"""World time from the clock file the platform writes at every advance (`clock.json`: `epoch_ms`, `world_ms`,
`speed`, `wall_ms`), read the way the apps' `platform_lib.clock` reads it: between advances time runs on at
`speed`, never more than `MAX_LEAD_MS` past the last advance (a paused world stands still) and never back.
`clock_ms` is world time since the world's epoch, what the schedule runs on. Without the file (a laptop, tests)
world time is wall time and the epoch is when the clock was made."""
import json
import threading
import time
from dataclasses import dataclass
from pathlib import Path

MAX_LEAD_MS = 1_000


@dataclass(frozen=True)
class Now:
    world_ms: int
    clock_ms: int


class Clock:
    def __init__(self, path: Path, wall=time.time):
        self.path, self.wall = Path(path), wall
        self._start_ms = self._wall_ms()
        self._seen: tuple[int, dict] | None = None
        self._last_ms = 0
        self._lock = threading.Lock()

    def now(self) -> Now:
        wall_ms, doc = self._wall_ms(), self._read()
        if doc is None:
            return Now(wall_ms, wall_ms - self._start_ms)
        lead = min(int((wall_ms - doc["wall_ms"]) * doc.get("speed", 1.0)), MAX_LEAD_MS)
        with self._lock:
            self._last_ms = max(self._last_ms, doc["world_ms"] + max(lead, 0))
            return Now(self._last_ms, self._last_ms - doc["epoch_ms"])

    def source(self) -> str:
        return "file" if self._read() is not None else "wall"

    def _wall_ms(self) -> int:
        return int(self.wall() * 1000)

    def _read(self) -> dict | None:
        try:
            mtime = self.path.stat().st_mtime_ns
            if self._seen is None or self._seen[0] != mtime:
                self._seen = (mtime, json.loads(self.path.read_text(encoding="utf-8")))
            return self._seen[1]
        except (OSError, ValueError):
            return self._seen[1] if self._seen else None
