import json

from conftest import Route

from traffic.clock import Now
from traffic.config import load
from traffic.otlp import Telemetry
from traffic.runner import Traffic
from traffic.schedule import plan

EPOCH = 1_773_122_400_000
CONTRACT = {"consumer": "web", "provider": "shop",
            "calls": [{"method": "GET", "path": "/stock/1", "fields": {"sku": "string"}}]}


class FakeClock:
    def __init__(self):
        self.clock_ms = 0

    def now(self) -> Now:
        return Now(EPOCH + self.clock_ms, self.clock_ms)

    def source(self) -> str:
        return "file"


def data(base: str, **changes) -> dict:
    return {"seed": 3, "tick_ms": 200, "timeout_s": 2, "workers": 1, "max_pending": 1000, "retention_minutes": 120,
            "export_every_s": 15, "user_agent": "t/1", "environment": "production",
            "services": {"shop": base}, "slo": {"window_minutes": 30, "objective": 0.01},
            "contracts": {"every_s": 60}, "day": {"start_hour": 9, "hourly": [1.0] * 24},
            "journeys": {"browse": {"rpm": 30, "steps": [
                {"method": "GET", "service": "shop", "path": "/products",
                 "pick": {"sku": {"from": "/products", "where": "n", "take": "/sku"}}},
                {"method": "GET", "service": "shop", "path": "/stock/{sku}"}]}}, **changes}


def traffic(tmp_path, base: str, **changes) -> tuple[Traffic, FakeClock]:
    (tmp_path / "journeys.json").write_text(json.dumps(data(base, **changes)))
    (tmp_path / "web").mkdir(exist_ok=True)
    (tmp_path / "web" / "shop.json").write_text(json.dumps(CONTRACT))
    config = load(tmp_path / "journeys.json", {"TRAFFIC_CONTRACTS": str(tmp_path)})
    clock = FakeClock()
    return Traffic(config, clock, Telemetry({"service.name": "traffic"}, None)), clock


SHOP = {"/products": {"products": [{"sku": "1", "n": 2}, {"sku": "2", "n": 5}]},
        "/stock/1": {"sku": "1"}, "/stock/2": {"sku": "2"}}


def journeys_seen(seen) -> int:
    return sum(r["path"] == "/products" for r in seen)


def test_a_tick_runs_the_journeys_planned_since_the_last_one(serve, tmp_path):
    base, seen = serve(SHOP)
    t, clock = traffic(tmp_path, base)
    t.tick()
    clock.clock_ms = 300_000
    t.tick()
    t.wait()
    planned = plan(3, 0, 300_000, t.config.profile)
    assert journeys_seen(seen) == len(planned) > 100
    assert t.stats_doc()["requests"] == 2 * len(planned) and t.stats_doc()["journeys"]["completed"] == len(planned)


def test_no_requests_while_the_clock_stands_still(serve, tmp_path):   # Review Focus 3
    base, seen = serve(SHOP)
    t, clock = traffic(tmp_path, base)
    clock.clock_ms = 120_000
    t.tick()
    for _ in range(5):
        t.tick()
    t.wait()
    assert journeys_seen(seen) == 0


def test_requests_are_recorded_at_world_time_with_a_trace_the_apps_continue(serve, tmp_path):
    base, seen = serve(SHOP)
    t, clock = traffic(tmp_path, base)
    t.tick()
    clock.clock_ms = 60_000
    t.tick()
    t.wait()
    assert t.stats.window(EPOCH + 60_000, 60_000)["requests"] == t.stats_doc()["requests"] > 0
    first = next(r for r in seen if r["path"] == "/products")
    assert first["headers"]["traceparent"].startswith("00-") and first["headers"]["user-agent"] == "t/1"


def test_contracts_run_at_start_and_every_minute_of_world_time(serve, tmp_path):
    base, seen = serve(SHOP)
    t, clock = traffic(tmp_path, base, journeys={})
    for ms in (0, 30_000, 59_000, 60_000, 60_000, 150_000):
        clock.clock_ms = ms
        t.tick()
    t.wait()
    assert sum(r["path"] == "/stock/1" for r in seen) == 3      # at 0, 60 s and 150 s
    assert t.contracts_doc() == {"web-shop": {"passed": True, "failures": [], "at_world_ms": EPOCH + 150_000}}


def test_before_the_first_run_a_contract_is_neither_passed_nor_failed(tmp_path, closed_port):
    t, _ = traffic(tmp_path, closed_port)
    assert t.contracts_doc() == {"web-shop": {"passed": None, "failures": [], "at_world_ms": None}}


def test_a_full_backlog_drops_journeys_and_counts_them(serve, tmp_path):
    base, _ = serve({**SHOP, "/products": Route(SHOP["/products"], delay_s=0.3)})
    t, clock = traffic(tmp_path, base, max_pending=1, contracts={"every_s": 100_000})
    t.tick()
    clock.clock_ms = 120_000
    t.tick()
    t.wait()
    doc = t.stats_doc()
    assert doc["journeys"]["dropped"] == len(plan(3, 0, 120_000, t.config.profile)) - 1 > 0


def test_the_same_seed_and_clock_send_the_same_requests(serve, tmp_path):
    runs = []
    for n in range(2):
        base, seen = serve(SHOP)
        (tmp_path / str(n)).mkdir()
        t, clock = traffic(tmp_path / str(n), base)
        t.tick()
        clock.clock_ms = 200_000
        t.tick()
        t.wait()
        runs.append([r["path"] for r in seen if "traceparent" in r["headers"]])   # not the contract probe's
    assert runs[0] == runs[1]
