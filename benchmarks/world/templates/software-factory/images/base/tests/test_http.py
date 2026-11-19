from fastapi import FastAPI
from fastapi.testclient import TestClient

from platform_lib.http import add_platform_routes


def test_health_and_version():
    healthy = [True]
    app = FastAPI()
    add_platform_routes(app, {"sha": "abc", "deps": {}}, healthy=lambda: healthy[0])
    client = TestClient(app)
    assert (client.get("/healthz").status_code, client.get("/healthz").json()) == (200, {"status": "ok"})
    assert client.get("/version").json() == {"sha": "abc", "deps": {}}
    healthy[0] = False
    assert (client.get("/healthz").status_code, client.get("/healthz").json()) == (503, {"status": "unavailable"})
