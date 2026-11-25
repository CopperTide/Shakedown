import os
from pathlib import Path

import httpx

from platform_lib import Runtime, serve

from .app import create_app
from .store import PgCarts
from .upstream import InventoryClient, OrdersClient

ROOT = Path(__file__).resolve().parents[1]
UPSTREAM_TIMEOUT_S = 2.0


def app_for(runtime: Runtime):
    inventory = InventoryClient(httpx.Client(base_url=os.environ["INVENTORY_URL"], timeout=UPSTREAM_TIMEOUT_S))
    orders = OrdersClient(httpx.Client(base_url=os.environ["ORDERS_URL"], timeout=UPSTREAM_TIMEOUT_S))
    return create_app(PgCarts(runtime.pool, runtime.metrics), inventory, orders)


if __name__ == "__main__":
    serve("storefront", ROOT, app_for)
