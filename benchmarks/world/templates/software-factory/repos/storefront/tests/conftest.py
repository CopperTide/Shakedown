import os

import pytest
from fakes import FakeInventory, FakeOrders, MemoryCarts, client_for
from fastapi.testclient import TestClient

from storefront.app import create_app
from storefront.upstream import InventoryClient, OrdersClient


@pytest.fixture(params=["memory", "postgres"])
def carts(request):
    """Each test runs on in-memory carts, and on Postgres too when TEST_DATABASE_URL names an empty database."""
    if request.param == "memory":
        yield MemoryCarts()
        return
    url = os.environ.get("TEST_DATABASE_URL")
    if not url:
        pytest.skip("TEST_DATABASE_URL is not set")
    from pg import postgres_carts
    with postgres_carts(url) as carts:
        yield carts


@pytest.fixture
def inventory():
    return FakeInventory()


@pytest.fixture
def orders(inventory):
    return FakeOrders(inventory)


@pytest.fixture
def client(carts, inventory, orders):
    app = create_app(carts, InventoryClient(client_for(inventory, "http://inventory.test")),
                     OrdersClient(client_for(orders, "http://orders.test")))
    return TestClient(app, raise_server_exceptions=False)
