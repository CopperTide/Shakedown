"""The platform's compose project and the observability stack's files, read without starting anything (Docker is
used only to have `docker compose config` check the file)."""
import json
import os
import shutil
import subprocess

import pytest
from sf_support import compose_limits, images_env, interpolate, load_yaml, metric_names, profile_env, services

DASHBOARDS = {"service-overview", "production-slo", "deploys", "database"}
ALERTS = {"HighErrorRate": "page", "HighLatency": "ticket", "ServiceDown": "page"}
GO_PROGRAMS = {"prometheus", "alertmanager", "loki", "tempo"}


def emitted(platform) -> set:
    return set((platform / "metrics.txt").read_text(encoding="utf-8").split())


def dashboard_exprs(path) -> list:
    board = json.loads(path.read_text(encoding="utf-8"))
    return [t["expr"] for p in board["panels"] for t in p.get("targets", []) if t.get("expr")]


def rules(platform) -> list:
    return [r for g in load_yaml(platform / "prometheus" / "rules.yml")["groups"] for r in g["rules"]]


@pytest.mark.parametrize("profile", ["lean", "full"])
def test_compose_config_validates(platform, sf_dir, tmp_path, profile):
    if shutil.which("docker") is None or subprocess.run(["docker", "compose", "version"],
                                                        capture_output=True).returncode != 0:
        pytest.skip("docker compose is not installed")
    env = {**os.environ, **profile_env(platform, profile), **images_env(sf_dir), "SHAKEDOWN_CLOCK_DIR": str(tmp_path)}
    argv = ["docker", "compose", "-p", "sf-config-check", "-f", str(platform / "compose.yaml"),
            *(["--profile", "full"] if profile == "full" else []), "config", "--quiet"]
    done = subprocess.run(argv, env=env, capture_output=True, text=True)
    assert done.returncode == 0, done.stderr


def test_every_lean_service_has_its_memory_limit_and_the_sum_fits(platform):
    budget = load_yaml(platform / "budget.yaml")
    limits, one_shot = compose_limits(platform, "lean"), budget["one_shot"]
    assert {n: v for n, v in limits.items() if n not in one_shot} == budget["containers"]
    assert {n: limits[n] for n in one_shot} == one_shot
    total = sum(budget["containers"].values()) + sum(budget["outside_compose"].values()) + \
        sum(budget["reserved"].values())
    assert total == budget["containers_mib"] and total <= 2600


def test_one_shot_containers_are_capped_and_counted_apart(platform):
    budget, lean = load_yaml(platform / "budget.yaml"), services(platform, "lean")
    one_shot = {n for n, s in lean.items() if s.get("restart") == "no"}
    assert one_shot == set(budget["one_shot"]) and not one_shot & set(budget["containers"])


def test_the_full_profile_adds_services_and_lifts_limits(platform):
    lean, full = compose_limits(platform, "lean"), compose_limits(platform, "full")
    assert set(lean) < set(full)
    assert all(full[n] >= lean[n] for n in lean) and any(full[n] > lean[n] for n in lean)
    assert sum(full.values()) <= 6 * 1024


def test_each_go_program_is_told_a_soft_limit_below_its_hard_one(platform):
    for profile in ("lean", "full"):
        env, limits = profile_env(platform, profile), compose_limits(platform, profile)
        for name in GO_PROGRAMS:
            soft = interpolate(services(platform, profile)[name]["environment"]["GOMEMLIMIT"], env)
            assert soft.endswith("MiB") and int(soft[:-3]) < limits[name], (profile, name)


def test_dashboards_query_only_metrics_the_apps_emit(platform):
    boards = {p.stem: p for p in (platform / "grafana" / "dashboards").glob("*.json")}
    assert set(boards) == DASHBOARDS
    for name, path in boards.items():
        read = set().union(*map(metric_names, dashboard_exprs(path)))
        assert read and read <= emitted(platform), (name, read - emitted(platform))


def test_alert_rules_read_real_metrics_with_the_planned_severities(platform):
    by_name = {r["alert"]: r for r in rules(platform)}
    assert {n: r["labels"]["severity"] for n, r in by_name.items()} == ALERTS
    for r in by_name.values():
        assert metric_names(r["expr"]) and metric_names(r["expr"]) <= emitted(platform), r["alert"]


def test_grafana_reads_every_store_through_the_gateway_in_wall_shift(platform):
    sources = load_yaml(platform / "grafana" / "provisioning" / "datasources" / "datasources.yaml")["datasources"]
    stores = {s["type"]: s for s in sources if s["type"] in {"prometheus", "loki", "tempo"}}
    assert set(stores) == {"prometheus", "loki", "tempo"}
    for kind, s in stores.items():
        assert s["url"] == f"http://worldclock-gateway:4319/q/{kind}"
        headers = {s["jsonData"][k]: s["secureJsonData"][k.replace("Name", "Value")]
                   for k in s["jsonData"] if k.startswith("httpHeaderName")}
        assert headers == {"X-Shakedown-Shift": "wall"}


def test_telemetry_goes_collector_then_gateway_then_stores(platform):
    collector = load_yaml(platform / "otel" / "collector.yaml")
    assert collector["exporters"]["otlphttp"]["endpoint"] == "http://worldclock-gateway:4318"
    assert collector["processors"]["memory_limiter"]["limit_mib"] == 80
    assert collector["processors"]["batch"]["send_batch_size"] == 512
    gateway = services(platform, "lean")["worldclock-gateway"]["environment"]
    assert gateway["METRICS_URL"] == "http://prometheus:9090/api/v1/otlp/v1/metrics"
    assert gateway["LOGS_URL"] == "http://loki:3100/otlp/v1/logs"
    assert gateway["TRACES_URL"] == "http://tempo:4318/v1/traces"


def test_every_app_container_sends_telemetry_and_errors_and_reads_the_clock(platform):
    apps = {n: s for n, s in services(platform, "lean").items() if n.split("-", 1)[0] in {"prod", "staging"}
            and not n.endswith("-db") and s.get("restart") != "no"}
    assert len(apps) == 6
    for name, s in apps.items():
        env = s["environment"]
        assert env["OTEL_EXPORTER_OTLP_ENDPOINT"] == "http://otel-collector:4318", name
        assert env["SENTRY_DSN"].startswith("http://") and "@glitchtip:8000/" in env["SENTRY_DSN"], name
        assert "${SHAKEDOWN_CLOCK_DIR" in s["volumes"][0] and s["volumes"][0].endswith(":/shakedown:ro"), name
        assert s["image"] == f"${{IMAGE_{name.upper().replace('-', '_')}:?set by the service part}}", name
        assert "healthcheck" in s, name


def test_every_image_compose_names_is_pinned_in_images_yaml(platform, sf_dir):
    named = images_env(sf_dir)
    for name, s in services(platform, "full").items():
        var = s["image"].removeprefix("${").split(":", 1)[0]
        assert var in named, name
    for image in load_yaml(sf_dir / "images.yaml")["images"]:
        ref = image["image"]
        if image.get("build"):
            assert "@" not in ref and image["hint"].split()[0].endswith(".sh"), ref
        else:
            name, digest = ref.split("@")
            assert ":" in name and digest.startswith("sha256:") and len(digest) == 71, ref
            assert image["hint"] == f"docker pull {ref}", ref


def test_volumes_are_saved_as_planned_and_probes_read_production(platform):
    part = load_yaml(platform / "service.yaml")
    saves = {v["name"]: (v["save"], v.get("live_copy", False)) for v in part["volumes"]}
    assert saves == {"prod-db": ("postgres", False), "staging-db": ("postgres", False),
                     "glitchtip-db": ("postgres", False), "prometheus": ("hardlink", True),
                     "loki": ("hardlink", True), "tempo": ("hardlink", True),
                     "grafana": ("skip", False), "alertmanager": ("skip", False)}
    assert set(saves) == set(load_yaml(platform / "compose.yaml")["volumes"])
    keys = {p["key"] for p in part["probes"]}
    assert {"alerts", "glitchtip/issues", "version/storefront", "version/orders", "version/inventory",
            "slo/production"} <= keys
    compose = load_yaml(platform / "compose.yaml")["services"]
    assert {p["service"] for p in part["probes"]} <= set(compose)
    assert {w["probe"] for w in part["watches"]} == {"alerts/paging"} <= keys


def test_the_part_starts_each_service_on_the_image_images_yaml_pins(platform, sf_dir):
    part, named = load_yaml(platform / "service.yaml"), images_env(sf_dir)
    assert set(part["images"]) == set(services(platform, "full"))
    assert {f"IMAGE_{s.upper().replace('-', '_')}": i for s, i in part["images"].items()} == named
    assert part["helper_image"] in {i["image"] for i in load_yaml(sf_dir / "images.yaml")["images"]}


def test_paging_alerts_carry_a_severity_pagerduty_accepts(platform):
    for r in rules(platform):
        assert r["labels"]["pagerduty_severity"] in {"critical", "error", "warning", "info"}
        assert r["annotations"]["summary"]


def test_every_app_reports_to_the_glitchtip_project_init_makes(platform):
    lean = services(platform, "lean")
    projects = dict(p.split("=") for p in lean["glitchtip"]["environment"]["SHOP_PROJECTS"].split(","))
    for number, (name, key) in enumerate(projects.items(), 1):
        for env in ("prod", "staging"):
            assert lean[f"{env}-{name}"]["environment"]["SENTRY_DSN"] == f"http://{key}@glitchtip:8000/{number}"
