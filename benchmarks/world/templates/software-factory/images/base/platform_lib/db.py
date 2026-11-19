"""The service's connection pool, and what it tells about its database's health and lock waits."""
import logging

from psycopg_pool import ConnectionPool

from .settings import Settings

log = logging.getLogger(__name__)
POOL_TIMEOUT_S = 2.0
LONGEST_LOCK_WAIT = ("SELECT coalesce(max(extract(epoch FROM now() - query_start)), 0) FROM pg_stat_activity "
                     "WHERE datname = current_database() AND wait_event_type = 'Lock'")


def connection_options(schema: str) -> dict:
    """Each service keeps its tables in a schema of its own in the shared database."""
    return {"options": f"-c search_path={schema}"}


def open_pool(settings: Settings) -> ConnectionPool:
    pool = ConnectionPool(settings.database_url, kwargs=connection_options(settings.db_schema), min_size=1,
                          max_size=settings.pool_size, timeout=POOL_TIMEOUT_S, name=settings.service, open=False)
    pool.open(wait=True, timeout=60)
    return pool


def reachable(pool: ConnectionPool) -> bool:
    try:
        with pool.connection(timeout=POOL_TIMEOUT_S) as conn:
            conn.execute("SELECT 1")
        return True
    except Exception as e:  # the health check reports any failure as unhealthy
        log.warning("database check failed: %s", e)
        return False


def longest_lock_wait_s(pool: ConnectionPool) -> float:
    try:
        with pool.connection(timeout=POOL_TIMEOUT_S) as conn:
            return float(conn.execute(LONGEST_LOCK_WAIT).fetchone()[0])
    except Exception as e:  # a sample that cannot be taken is skipped
        log.warning("lock wait sample failed: %s", e)
        return 0.0
