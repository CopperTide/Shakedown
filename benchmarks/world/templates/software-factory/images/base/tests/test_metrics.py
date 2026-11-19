from pathlib import Path

from fastapi import FastAPI, HTTPException
from fastapi.testclient import TestClient
from support import SETTINGS, collected, prometheus_names, reading_meter

from platform_lib.metrics import SECONDS_BUCKETS, Metrics, RequestMetrics

METRICS_TXT = Path(__file__).resolve().parents[3] / "platform" / "metrics.txt"


def app_with(metrics: Metrics) -> FastAPI:
    app = FastAPI()

    @app.get("/orders/{order_id}")
    def get_order(order_id: int):
        if order_id == 0:
            raise HTTPException(404, "no such order")
        if order_id < 0:
            raise RuntimeError("the database went away")
        return {"id": order_id}

    app.add_middleware(RequestMetrics, metrics=metrics)
    return app


def points(reader, name: str) -> list:
    return [p for m in collected(reader) if m.name == name for p in m.data.data_points]


def test_requests_are_measured_by_route_method_and_status():
    meter, reader = reading_meter()
    client = TestClient(app_with(Metrics(meter, SETTINGS)), raise_server_exceptions=False)
    assert [client.get(p).status_code for p in ("/orders/7", "/orders/0", "/orders/-1", "/nope")] == [200, 404, 500, 404]
    seen = {(p.attributes["http.route"], p.attributes["http.response.status_code"]): p.count
            for p in points(reader, "http.server.request.duration")}
    assert seen == {("/orders/{order_id}", 200): 1, ("/orders/{order_id}", 404): 1, ("/orders/{order_id}", 500): 1,
                    ("unmatched", 404): 1}
    assert tuple(points(reader, "http.server.request.duration")[0].explicit_bounds) == SECONDS_BUCKETS


def test_queries_deploys_and_lock_waits_are_measured():
    meter, reader = reading_meter()
    metrics = Metrics(meter, SETTINGS, lock_wait_s=lambda: 2.5)
    with metrics.query("insert_order"):
        pass
    assert [p.attributes for p in points(reader, "db.query.duration")] == [{"db.operation": "insert_order"}]
    assert [(p.value, dict(p.attributes)) for p in points(reader, "deploy.info")] == [(1, {"sha": "4f2a9c1"})]
    assert [p.value for p in points(reader, "db.lock.wait")] == [2.5]


def test_what_a_service_emits_is_what_metrics_txt_lists():
    meter, reader = reading_meter()
    metrics = Metrics(meter, SETTINGS, lock_wait_s=lambda: 0.0)
    TestClient(app_with(metrics)).get("/orders/7")
    with metrics.query("select"):
        pass
    names = set().union(*map(prometheus_names, collected(reader)))
    assert names == set(METRICS_TXT.read_text(encoding="utf-8").split())
