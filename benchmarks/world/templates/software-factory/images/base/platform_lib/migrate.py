"""Schema migrations: the `.sql` files of a folder, run in name order, each once and in its own transaction,
recorded in the service schema's `schema_migrations`. An advisory lock keeps two starting containers from running
them together."""
import logging
from pathlib import Path

from psycopg import sql

log = logging.getLogger(__name__)
LOCK_ID = 7_301_966


def pending(folder: Path, applied: set[str]) -> list[Path]:
    return sorted((p for p in folder.glob("*.sql") if p.stem not in applied), key=lambda p: p.name)


def migrate(pool, folder: Path, schema: str | None = None) -> list[str]:
    """Run the folder's pending migrations (in `schema`, made if missing, when given); the names run."""
    with pool.connection() as conn:
        conn.autocommit = True
        conn.execute("SELECT pg_advisory_lock(%s)", (LOCK_ID,))
        try:
            if schema:
                conn.execute(sql.SQL("CREATE SCHEMA IF NOT EXISTS {}").format(sql.Identifier(schema)))
            conn.execute("CREATE TABLE IF NOT EXISTS schema_migrations "
                         "(version text PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now())")
            applied = {r[0] for r in conn.execute("SELECT version FROM schema_migrations")}
            return [_run(conn, path) for path in pending(folder, applied)]
        finally:
            conn.execute("SELECT pg_advisory_unlock(%s)", (LOCK_ID,))
            conn.autocommit = False


def _run(conn, path: Path) -> str:
    log.info("applying migration %s", path.name)
    with conn.transaction():
        conn.execute(path.read_text(encoding="utf-8"))
        conn.execute("INSERT INTO schema_migrations (version) VALUES (%s)", (path.stem,))
    return path.stem
