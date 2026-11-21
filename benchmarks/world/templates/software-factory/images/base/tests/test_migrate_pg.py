"""Migrations on a real database, when TEST_DATABASE_URL names one (it is left as it was)."""
import os

import pytest
from psycopg_pool import ConnectionPool

from platform_lib.db import connection_options
from platform_lib.migrate import migrate

URL = os.environ.get("TEST_DATABASE_URL")
pytestmark = pytest.mark.skipif(not URL, reason="TEST_DATABASE_URL is not set")


def test_migrations_run_once_in_the_services_schema(tmp_path):
    (tmp_path / "001_create.sql").write_text("CREATE TABLE things (id int PRIMARY KEY);")
    (tmp_path / "002_seed.sql").write_text("INSERT INTO things VALUES (1), (2);")
    with ConnectionPool(URL, kwargs=connection_options("pl_test"), min_size=1, max_size=2, open=True) as pool:
        try:
            assert migrate(pool, tmp_path, "pl_test") == ["001_create", "002_seed"]
            assert migrate(pool, tmp_path, "pl_test") == []
            with pool.connection() as conn:
                assert conn.execute("SELECT count(*) FROM pl_test.things").fetchone()[0] == 2
        finally:
            with pool.connection() as conn:
                conn.execute("DROP SCHEMA IF EXISTS pl_test CASCADE")
