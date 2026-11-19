"""The traffic service as template data: wired into the platform's compose project within its 64 MiB on the pinned
python image, its probes the docs scoring reads, an unchanged copy of the consumers' contracts, data that names
production's services, the traffic-peak event the builder toggles, and the same wording and clock as the apps'
platform_lib."""
import importlib.util
import json
import sys
from pathlib import Path

import jsonschema
import pytest
from conftest import REPOS, TEMPLATE
from support import compose_limits, images_env, load_yaml, profile_env, services

sys.path.insert(0, str(TEMPLATE / "platform"))
from traffic.clock import Clock  # noqa: E402
from traffic.config import load  # noqa: E402
from traffic.contracts import field_failures  # noqa: E402

LIB = TEMPLATE / "images" / "base" / "platform_lib"


def lib_module(name: str):
    """A platform_lib module by its file, without the package's OpenTelemetry imports."""
    spec = importlib.util.spec_from_file_location(f"platform_lib_{name}", LIB / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def traffic_service(platform) -> dict:
    return services(platform, "lean")["traffic"]


def test_the_traffic_runs_in_compose_on_the_pinned_python_image_within_64_mib(platform, sf_dir):
    s = traffic_service(platform)
    assert compose_limits(platform, "lean")["traffic"] == 64 and compose_limits(platform, "full")["traffic"] >= 64
    assert s["image"] == "${IMAGE_TRAFFIC:?set by the service part}"
    pinned = {i["id"]: i for i in load_yaml(sf_dir / "images.yaml")["images"]}["python"]
    assert "traffic" in pinned["services"] and "@sha256:" in pinned["image"]
    assert images_env(sf_dir)["IMAGE_TRAFFIC"] == load_yaml(platform / "service.yaml")["images"]["traffic"]
    assert s["command"] == ["python", "-m", "traffic"] and s["working_dir"] == "/app"
    assert set(s["volumes"]) == {"${ARENA_CLOCK_DIR:?set by the service part}:/shakedown:ro",
                                 "./traffic:/app/traffic:ro", "./contracts:/app/contracts:ro"}
    assert s["ports"] == ["127.0.0.1::9100"] and s["environment"]["OTEL_EXPORTER_OTLP_ENDPOINT"] == \
        "http://otel-collector:4318"


def test_the_traffic_starts_once_production_is_up_and_seeded(platform):
    after = traffic_service(platform)["depends_on"]
    assert after == {"prod-storefront": {"condition": "service_healthy"},
                     "prod-orders": {"condition": "service_healthy"},
                     "prod-inventory": {"condition": "service_healthy"},
                     "prod-seed": {"condition": "service_completed_successfully"}}


def test_the_scoring_probes_read_the_traffic_service(platform):
    probes = {p["key"]: p for p in load_yaml(platform / "service.yaml")["probes"]}
    assert {k: (p["service"], p["port"], p["path"]) for k, p in probes.items() if p["service"] == "traffic"} == {
        "slo/production": ("traffic", 9100, "/slo?time={world_s}"), "traffic/stats": ("traffic", 9100, "/stats"),
        "contracts": ("traffic", 9100, "/contracts")}


def test_the_contract_probe_runs_an_unchanged_copy_of_each_consumers_contracts(sf_dir, platform):
    schema = json.loads((sf_dir / "contracts.schema.json").read_text())
    repo_files = {f"{r}/{p.name}": p for r in REPOS for p in (sf_dir / "repos" / r / "contracts").glob("*.json")}
    copies = {f"{p.parent.name}/{p.name}": p for p in (platform / "contracts").glob("*/*.json")}
    assert set(copies) == set(repo_files) == {"storefront/inventory.json", "storefront/orders.json",
                                              "orders/inventory.json"}
    for name, path in copies.items():
        assert path.read_bytes() == repo_files[name].read_bytes(), name
        jsonschema.validate(json.loads(path.read_text()), schema)


def test_the_traffic_data_loads_and_sends_only_to_production(platform):
    config = load(platform / "traffic" / "journeys.json", {})
    compose = services(platform, "lean")
    for name, url in config.services.items():
        host, port = url.removeprefix("http://").split(":")
        assert host.startswith("prod-") and host in compose and port == "8000", name
    assert set(config.journeys) == {"browse", "order", "check-stock"} and config.slo == {"window_minutes": 30,
                                                                                         "objective": 0.01}
    assert config.contracts_dir == Path("/app/contracts") and config.clock_file == Path("/shakedown/clock.json")


def test_the_traffic_peak_is_data_the_builder_toggles(sf_dir, platform):
    event = load_yaml(sf_dir / "events" / "traffic-peak.yaml")
    assert event["id"] == "traffic-peak" and event["default"] is False
    (apply,) = event["apply"]
    assert (apply["part"], apply["set"]) == ("platform", "env.TRAFFIC_EVENTS")
    params = {p["name"]: p["default"] for p in event["params"]}
    toggles = apply["value"].replace("{param.at_minute}", str(params["at_minute"]))
    peak = load(platform / "traffic" / "journeys.json", {"TRAFFIC_EVENTS": toggles}).profile["peaks"]
    assert peak == [{"from_ms": params["at_minute"] * 60_000, "to_ms": (params["at_minute"] + 30) * 60_000,
                     "factor": 3}]
    assert load_yaml(platform / "service.yaml")["env"]["TRAFFIC_EVENTS"] == ""
    assert traffic_service(platform)["environment"]["TRAFFIC_EVENTS"] == "${TRAFFIC_EVENTS:-}"


CASES = [({"total": 12, "status": "x", "lines": []}), ({"total_cents": 1, "status": 3, "lines": {}}),
         ({"total": True, "status": None, "lines": []}), [1], "text", None]


@pytest.mark.parametrize("body", CASES)
def test_the_probe_words_contract_failures_as_the_apps_tests_do(body):
    call = {"method": "GET", "path": "/orders/1", "fields": {"total": "number", "status": "string", "lines": "array"}}
    assert field_failures(call, body) == lib_module("contracts").failures(call, body)


def test_the_traffic_reads_the_clock_as_the_apps_do(tmp_path):
    path, wall = tmp_path / "clock.json", [50.0]
    path.write_text(json.dumps({"epoch_ms": 0, "clock_ms": 60_000, "world_ms": 60_000, "speed": 2.0,
                                "wall_ms": 50_000}))
    ours, apps = Clock(path, lambda: wall[0]), lib_module("clock").WorldClock(path, lambda: wall[0])
    for wall[0] in (50.0, 50.2, 50.4, 51.0, 59.0):
        assert ours.now().world_ms == apps.now_ms()


def test_the_traffic_is_counted_in_the_lean_budget(platform):
    budget = load_yaml(platform / "budget.yaml")
    assert budget["containers"]["traffic"] == 64 and not budget["reserved"]
    assert profile_env(platform, "lean")["MEM_TRAFFIC"] == "64m"
