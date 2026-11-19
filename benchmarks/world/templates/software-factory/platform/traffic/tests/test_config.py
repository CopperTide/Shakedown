import json

import pytest

from traffic.config import ConfigError, load

DATA = {
    "seed": 1, "tick_ms": 200, "timeout_s": 2, "workers": 4, "max_pending": 8, "retention_minutes": 120,
    "export_every_s": 15, "user_agent": "t/1",
    "services": {"shop": "http://shop:8000"},
    "slo": {"window_minutes": 30, "objective": 0.01},
    "contracts": {"every_s": 60},
    "day": {"start_hour": 9, "hourly": [1.0] * 24},
    "journeys": {"browse": {"rpm": 10, "steps": [{"method": "GET", "service": "shop", "path": "/products"}]}},
    "events": {"traffic-peak": {"at_minute": 90, "for_minutes": 30, "factor": 3}},
}


def config(tmp_path, data=DATA, **env):
    path = tmp_path / "journeys.json"
    path.write_text(json.dumps(data))
    return load(path, env)


def test_the_profile_holds_each_journeys_rate_and_the_days_shape(tmp_path):
    c = config(tmp_path)
    assert c.seed == 1 and c.profile == {"journeys": {"browse": {"rpm": 10}}, "day": DATA["day"], "peaks": []}


def test_the_seed_and_events_come_from_the_environment_the_builder_sets(tmp_path):
    c = config(tmp_path, TRAFFIC_SEED="42", TRAFFIC_EVENTS="traffic-peak")
    assert c.seed == 42 and c.profile["peaks"] == [{"from_ms": 90 * 60_000, "to_ms": 120 * 60_000, "factor": 3}]
    moved = config(tmp_path, TRAFFIC_EVENTS=" traffic-peak@45 ")
    assert moved.profile["peaks"] == [{"from_ms": 45 * 60_000, "to_ms": 75 * 60_000, "factor": 3}]


def test_an_unknown_event_or_a_step_to_an_unknown_service_is_refused(tmp_path):
    with pytest.raises(ConfigError, match="no traffic event surge"):
        config(tmp_path, TRAFFIC_EVENTS="surge")
    bad = {**DATA, "journeys": {"x": {"rpm": 1, "steps": [{"method": "GET", "service": "nope", "path": "/"}]}}}
    with pytest.raises(ConfigError, match="journey x step 1 names the service nope"):
        config(tmp_path, bad)
    with pytest.raises(ConfigError, match="24 hourly"):
        config(tmp_path, {**DATA, "day": {"start_hour": 9, "hourly": [1.0]}})


def test_service_addresses_and_the_collector_can_be_overridden(tmp_path):
    c = config(tmp_path, TRAFFIC_SERVICES="shop=http://127.0.0.1:9", OTEL_EXPORTER_OTLP_ENDPOINT="http://c:4318")
    assert c.services == {"shop": "http://127.0.0.1:9"} and c.otlp_endpoint == "http://c:4318"
    assert config(tmp_path).otlp_endpoint is None
