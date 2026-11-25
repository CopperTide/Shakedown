"""Postgres carts on a test database: migrated and emptied."""
from contextlib import contextmanager
from pathlib import Path

from opentelemetry.metrics import NoOpMeterProvider
from psycopg_pool import ConnectionPool

from platform_lib.metrics import Metrics
from platform_lib.migrate import migrate
from platform_lib.settings import Settings
from storefront.store import PgCarts

MIGRATIONS = Path(__file__).resolve().parents[1] / "migrations"


@contextmanager
def postgres_carts(url: str):
    with ConnectionPool(url, min_size=1, max_size=4, open=True) as pool:
        migrate(pool, MIGRATIONS)
        with pool.connection() as conn:
            conn.execute("TRUNCATE cart_items, carts")
        yield PgCarts(pool, Metrics(NoOpMeterProvider().get_meter("test"), Settings.from_env("storefront")))
