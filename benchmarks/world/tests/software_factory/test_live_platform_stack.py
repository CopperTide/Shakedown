"""Live: the observability services of the platform's own compose file under the lean profile (no apps and no
gateway): each comes up within its limit, Grafana has its datasources and dashboards, and GlitchTip's init has
made the projects the apps report to and takes an event sent with an app's DSN."""
import json
import os
import shutil
import subprocess
import time
import urllib.request

import pytest
from sf_support import compose_limits, images_env, profile_env, services

pytestmark = pytest.mark.live
STACK = ["prometheus", "alertmanager", "loki", "tempo", "grafana", "glitchtip"]
PROJECT = "sf-live-obs"


def compose(platform, env, *argv, timeout=600) -> subprocess.CompletedProcess:
    return subprocess.run(["docker", "compose", "-p", PROJECT, "-f", str(platform / "compose.yaml"), *argv],
                          env=env, capture_output=True, text=True, timeout=timeout)


@pytest.fixture(scope="module")
def stack(sf_dir, platform, tmp_path_factory):
    if shutil.which("docker") is None:
        pytest.skip("docker is not installed")
    named = images_env(sf_dir)
    for service in [*STACK, "glitchtip-db"]:
        ref = named[f"IMAGE_{service.upper().replace('-', '_')}"]
        if subprocess.run(["docker", "image", "inspect", ref], capture_output=True).returncode != 0:
            pytest.skip(f"{ref} is not on this machine")
    env = {**os.environ, **profile_env(platform, "lean"), **named,
           "ARENA_CLOCK_DIR": str(tmp_path_factory.mktemp("clock"))}
    try:
        up = compose(platform, env, "up", "-d", "--pull", "never", "--wait", "--wait-timeout", "300", *STACK)
        assert up.returncode == 0, up.stdout[-2000:] + up.stderr[-2000:]
        yield env
    finally:
        compose(platform, env, "down", "-v", "--remove-orphans")


def url(platform, env, service: str, port: int) -> str:
    done = compose(platform, env, "port", service, str(port))
    return f"http://{done.stdout.strip()}"


def get(address: str, headers=None, data=None):
    request = urllib.request.Request(address, data=data, headers=headers or {})
    with urllib.request.urlopen(request, timeout=10) as r:
        return r.status, json.loads(r.read() or b"null")


def test_live_every_service_runs_under_its_limit(platform, stack):
    time.sleep(20)
    limits = compose_limits(platform, "lean")
    ps = [json.loads(line) for line in compose(platform, stack, "ps", "--format", "json").stdout.splitlines()]
    assert {p["Service"]: p["State"] for p in ps} == {s: "running" for s in [*STACK, "glitchtip-db"]}
    for p in ps:
        stats = subprocess.run(["docker", "stats", "--no-stream", "--format", "{{.MemUsage}}", p["Name"]],
                               capture_output=True, text=True).stdout
        print(p["Service"], stats.strip(), f"(limit {limits[p['Service']]} MiB)")


def test_live_prometheus_and_alertmanager_are_ready_with_the_rules(platform, stack):
    prometheus = url(platform, stack, "prometheus", 9090)
    status, body = get(f"{prometheus}/api/v1/rules")
    assert status == 200
    assert {r["name"] for g in body["data"]["groups"] for r in g["rules"]} == {"HighErrorRate", "HighLatency",
                                                                             "ServiceDown"}
    assert get(f"{url(platform, stack, 'alertmanager', 9093)}/api/v2/status")[0] == 200


def test_live_grafana_has_the_datasources_and_dashboards(platform, stack):
    grafana, auth = url(platform, stack, "grafana", 3000), {"Authorization": "Basic YWRtaW46YWRtaW4="}
    sources = get(f"{grafana}/api/datasources", auth)[1]
    assert {(s["name"], s["url"]) for s in sources} == {
        ("Prometheus", "http://worldclock-gateway:4319/q/prometheus"),
        ("Loki", "http://worldclock-gateway:4319/q/loki"), ("Tempo", "http://worldclock-gateway:4319/q/tempo")}
    boards = get(f"{grafana}/api/search?type=dash-db", auth)[1]
    assert {b["title"] for b in boards} == {"Service overview", "Production SLO", "Deploys", "Database"}


def test_live_glitchtip_has_the_apps_projects_and_takes_their_events(platform, stack):
    glitchtip, glitch = url(platform, stack, "glitchtip", 8000), services(platform, "lean")["glitchtip"]
    token = {"Authorization": f"Bearer {glitch['environment']['SHOP_API_TOKEN']}"}
    projects = get(f"{glitchtip}/api/0/organizations/shop/projects/", token)[1]
    assert [p["slug"] for p in sorted(projects, key=lambda p: int(p["id"]))] == ["storefront", "orders", "inventory"]
    dsn = services(platform, "lean")["prod-orders"]["environment"]["SENTRY_DSN"]
    key, project = dsn.removeprefix("http://").split("@")[0], dsn.rsplit("/", 1)[1]
    event = json.dumps({"event_id": "a" * 32, "message": "live check", "level": "error", "platform": "python",
                        "timestamp": time.time()}).encode()
    auth = {"X-Sentry-Auth": f"Sentry sentry_version=7, sentry_key={key}", "Content-Type": "application/json"}
    assert get(f"{glitchtip}/api/{project}/store/", auth, event)[0] == 200
