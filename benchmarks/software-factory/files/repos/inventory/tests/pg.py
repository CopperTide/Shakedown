"""A `PgStore` on a test database: migrated, emptied, and filled with the catalog."""
from contextlib import contextmanager
from pathlib import Path

from opentelemetry.metrics import NoOpMeterProvider
from psycopg_pool import ConnectionPool

from inventory.store import PgStore
from platform_lib.metrics import Metrics
from platform_lib.migrate import migrate
from platform_lib.settings import Settings

MIGRATIONS = Path(__file__).resolve().parents[1] / "migrations"


@contextmanager
def postgres_store(url: str, catalog):
    with ConnectionPool(url, min_size=1, max_size=4, open=True) as pool:
        migrate(pool, MIGRATIONS)
        with pool.connection() as conn:
            conn.execute("TRUNCATE reservation_lines, reservations, products")
            for p in catalog:
                conn.execute("INSERT INTO products VALUES (%s, %s, %s, %s, %s, %s)",
                             (p.sku, p.name, p.list_price, p.discount_pct, p.currency, p.on_hand))
        yield PgStore(pool, Metrics(NoOpMeterProvider().get_meter("test"), Settings.from_env("inventory")))
