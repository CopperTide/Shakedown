import os

import pytest
from fakes import CATALOG, MemoryStore, SteppedClock
from fastapi.testclient import TestClient

from inventory.app import create_app


@pytest.fixture(params=["memory", "postgres"])
def store(request):
    """Each test runs on the in-memory store, and on Postgres too when TEST_DATABASE_URL names an empty database."""
    if request.param == "memory":
        yield MemoryStore()
        return
    url = os.environ.get("TEST_DATABASE_URL")
    if not url:
        pytest.skip("TEST_DATABASE_URL is not set")
    from pg import postgres_store
    with postgres_store(url, CATALOG) as store:
        yield store


@pytest.fixture
def clock():
    return SteppedClock()


@pytest.fixture
def client(store, clock):
    return TestClient(create_app(store, clock, hold_seconds=900))
