"""Starting a service: settings from the environment, error reports and telemetry, the database pool and its
migrations, then the app the service builds, with the platform's routes and request metrics, under uvicorn."""
import logging
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

import uvicorn
from fastapi import FastAPI
from psycopg_pool import ConnectionPool

from . import db
from .clock import WorldClock
from .errors import setup_errors
from .http import add_platform_routes
from .metrics import Metrics, RequestMetrics
from .migrate import migrate
from .settings import Settings
from .telemetry import Telemetry
from .version import version_doc


@dataclass(frozen=True)
class Runtime:
    """What a service's app is built from."""
    settings: Settings
    clock: WorldClock
    pool: ConnectionPool
    metrics: Metrics


def build(service: str, root: Path, create_app: Callable[[Runtime], FastAPI]) -> tuple[FastAPI, Telemetry]:
    settings, clock = Settings.from_env(service), WorldClock()
    setup_errors(settings, clock)
    telemetry = Telemetry(settings)
    pool = db.open_pool(settings)
    migrate(pool, root / "migrations", settings.db_schema)
    metrics = Metrics(telemetry.meter(), settings, lock_wait_s=lambda: db.longest_lock_wait_s(pool))
    app = create_app(Runtime(settings, clock, pool, metrics))
    add_platform_routes(app, version_doc(root / "requirements.txt"), healthy=lambda: db.reachable(pool))
    app.add_middleware(RequestMetrics, metrics=metrics)
    telemetry.instrument(app)
    return app, telemetry


def serve(service: str, root: Path, create_app: Callable[[Runtime], FastAPI]) -> None:
    """Run `service` (its repository checked out at `root`) on `PORT` (8000)."""
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(name)s: %(message)s")
    app, telemetry = build(service, root, create_app)
    try:
        uvicorn.run(app, host="0.0.0.0", port=int(os.environ.get("PORT", "8000")), log_config=None)
    finally:
        telemetry.shutdown()
