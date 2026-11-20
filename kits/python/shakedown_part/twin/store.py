"""A twin's SQLite state: one transaction per request, saved and restored whole."""
import sqlite3
import threading
from contextlib import contextmanager
from pathlib import Path


class Store:
    """One connection shared by the twin's threads. `tx()` is re-entrant on a thread: an inner
    `tx()` joins the outer transaction, which commits or rolls back as a whole."""

    def __init__(self, path: Path, schema_sql: str = "", readonly: bool = False):
        uri = f"file:{Path(path)}?mode=ro" if readonly else f"file:{Path(path)}"
        self.conn = sqlite3.connect(uri, uri=True, isolation_level=None, check_same_thread=False)
        self.conn.row_factory = sqlite3.Row
        self.conn.execute("PRAGMA foreign_keys = ON")
        self._lock = threading.RLock()
        self._depth = 0
        if schema_sql and not readonly:
            self.conn.executescript(schema_sql)

    @contextmanager
    def tx(self):
        """BEGIN IMMEDIATE … COMMIT; ROLLBACK when the block raises."""
        with self._lock:
            if self._depth:
                self._depth += 1
                try:
                    yield self.conn
                finally:
                    self._depth -= 1
                return
            self.conn.execute("BEGIN IMMEDIATE")
            self._depth = 1
            try:
                yield self.conn
            except BaseException:
                self.conn.execute("ROLLBACK")
                raise
            else:
                self.conn.execute("COMMIT")
            finally:
                self._depth = 0

    def backup_to(self, path: Path) -> int:
        """Write a consistent copy to `path`; its size in bytes."""
        with self._lock, sqlite3.connect(Path(path)) as dst:
            self.conn.backup(dst)
        return Path(path).stat().st_size

    def restore_from(self, path: Path) -> None:
        """Replace the whole state with the copy at `path`."""
        with self._lock:
            src = sqlite3.connect(f"file:{Path(path)}?mode=ro", uri=True)
            try:
                src.backup(self.conn)
            finally:
                src.close()

    def close(self) -> None:
        with self._lock:
            self.conn.close()
