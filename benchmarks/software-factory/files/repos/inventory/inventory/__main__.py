import os
from pathlib import Path

from platform_lib import Runtime, serve

from .app import create_app
from .store import PgStore

ROOT = Path(__file__).resolve().parents[1]


def app_for(runtime: Runtime):
    store = PgStore(runtime.pool, runtime.metrics)
    return create_app(store, runtime.clock, hold_seconds=int(os.environ.get("HOLD_SECONDS", "900")))


if __name__ == "__main__":
    serve("inventory", ROOT, app_for)
