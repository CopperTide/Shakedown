"""The two routes every service answers: `/healthz` (for the container's health check) and `/version`."""
from typing import Callable

from fastapi import FastAPI
from fastapi.responses import JSONResponse


def add_platform_routes(app: FastAPI, version: dict, healthy: Callable[[], bool]) -> None:
    @app.get("/healthz", include_in_schema=False)
    def healthz():
        if healthy():
            return {"status": "ok"}
        return JSONResponse({"status": "unavailable"}, status_code=503)

    @app.get("/version")
    def get_version():
        return version
