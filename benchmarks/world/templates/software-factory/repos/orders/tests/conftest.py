import os

import pytest
from fakes import RESERVATION, FakeInventory, MemoryStore
from fastapi.testclient import TestClient

from orders.app import create_app
from orders.inventory import Inventory


class FixedClock:
    def now(self) -> float:
        return 1_773_122_400.0  # 2026-03-10T06:00:00Z


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
    with postgres_store(url) as store:
        yield store


@pytest.fixture
def inventory():
    return FakeInventory(RESERVATION, {**RESERVATION, "id": "r-old", "status": "expired"})


@pytest.fixture
def client(store, inventory):
    return TestClient(create_app(store, Inventory(inventory.client()), FixedClock()))
