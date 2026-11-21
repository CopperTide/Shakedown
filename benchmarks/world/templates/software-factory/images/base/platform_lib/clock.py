"""World time, read from the clock file the platform writes at every advance (`/shakedown/clock.json`:
`world_ms`, `wall_ms`, `speed`). Between advances world time runs on at `speed`, but never more than
`MAX_LEAD_MS` past the last advance, so a paused world stands still; it never goes back. Without the file
(tests, a laptop) world time is wall time."""
import json
import os
import threading
import time
from pathlib import Path

MAX_LEAD_MS = 1_000
DEFAULT_FILE = Path(os.environ.get("SHAKEDOWN_CLOCK_FILE", "/shakedown/clock.json"))


class WorldClock:
    def __init__(self, path: Path = DEFAULT_FILE, wall=time.time):
        self.path, self.wall = Path(path), wall
        self._seen: tuple[int, dict] | None = None
        self._last_ms = 0
        self._lock = threading.Lock()

    def now_ms(self) -> int:
        wall_ms = int(self.wall() * 1000)
        doc = self._read()
        if doc is None:
            return wall_ms
        lead = min(int((wall_ms - doc["wall_ms"]) * doc.get("speed", 1.0)), MAX_LEAD_MS)
        with self._lock:
            self._last_ms = max(self._last_ms, doc["world_ms"] + max(lead, 0))
            return self._last_ms

    def now(self) -> float:
        """World time in seconds since the epoch, as `time.time()` gives wall time."""
        return self.now_ms() / 1000

    def _read(self) -> dict | None:
        try:
            mtime = self.path.stat().st_mtime_ns
            if self._seen is None or self._seen[0] != mtime:
                self._seen = (mtime, json.loads(self.path.read_text(encoding="utf-8")))
            return self._seen[1]
        except (OSError, ValueError, KeyError):
            return self._seen[1] if self._seen else None
