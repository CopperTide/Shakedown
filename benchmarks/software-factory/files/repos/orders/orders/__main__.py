import os
from pathlib import Path

import httpx

from platform_lib import Runtime, serve

from .app import create_app
from .inventory import Inventory
from .store import PgStore

ROOT = Path(__file__).resolve().parents[1]


def app_for(runtime: Runtime):
    inventory = Inventory(httpx.Client(base_url=os.environ["INVENTORY_URL"], timeout=2.0))
    return create_app(PgStore(runtime.pool, runtime.metrics), inventory, runtime.clock)


if __name__ == "__main__":
    serve("orders", ROOT, app_for)
