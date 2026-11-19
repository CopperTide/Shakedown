"""Live: the traffic service from the platform's compose file against the real production containers (seeded
Postgres, storefront, orders, inventory) and the real collector, on a world clock this test writes at four
times wall speed: users browse, order and check stock without server errors, every contract holds, the SLO
probe reads the requests, the collector takes the traffic's OTLP, a paused clock stops the traffic, and the
container stays under its 64 MiB."""
import json
import os
import shutil
import subprocess
import threading
import time
import urllib.request

import pytest
from support import images_env, profile_env

pytestmark = pytest.mark.live
PROJECT = "sf-live-traffic"
SERVICES = ["traffic", "otel-collector"]          # compose starts production and its seed for the traffic
PULLED = ["IMAGE_PROD_DB", "IMAGE_OTEL_COLLECTOR", "IMAGE_TRAFFIC"]
BUILT = ["IMAGE_PROD_STOREFRONT", "IMAGE_PROD_ORDERS", "IMAGE_PROD_INVENTORY"]
SPEED = 4.0
EPOCH = 1_773_122_400_000                         # 2026-03-10T06:00Z


def present(ref: str) -> bool:
    return subprocess.run(["docker", "image", "inspect", ref], capture_output=True).returncode == 0


def compose(platform, env, *argv, timeout=600) -> subprocess.CompletedProcess:
    return subprocess.run(["docker", "compose", "-p", PROJECT, "-f", str(platform / "compose.yaml"), *argv],
                          env=env, capture_output=True, text=True, timeout=timeout)


class WorldClock(threading.Thread):
    """Writes clock.json as the platform part does at each advance, the world running at SPEED."""

    def __init__(self, folder):
        super().__init__(daemon=True)
        self.path, self.clock_ms, self.running, self.stopped = folder / "clock.json", 0, threading.Event(), False
        self.running.set()
        self.write()

    def write(self):
        doc = {"epoch_ms": EPOCH, "clock_ms": self.clock_ms, "world_ms": EPOCH + self.clock_ms, "speed": SPEED,
               "wall_ms": int(time.time() * 1000)}
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(json.dumps(doc))
        tmp.chmod(0o644)
        tmp.replace(self.path)

    def run(self):
        while not self.stopped:
            time.sleep(0.25)
            if self.running.is_set():
                self.clock_ms += int(250 * SPEED)
                self.write()


def ensure_images(sf_dir, named):
    missing = [named[k] for k in PULLED if not present(named[k])]
    if missing:
        pytest.skip(f"not on this machine (images.yaml has the pull hints): {', '.join(missing)}")
    if not all(present(named[k]) for k in BUILT):
        done = subprocess.run([str(sf_dir / "images" / "build.sh"), "all"], capture_output=True, text=True,
                              timeout=1800)
        assert done.returncode == 0, done.stdout[-3000:] + done.stderr[-3000:]


def get(url: str):
    with urllib.request.urlopen(url, timeout=5) as r:
        return json.loads(r.read())


def wait_for(fn, seconds: float, what: str):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = fn()
            if value:
                return value
        except OSError:
            pass
        time.sleep(1)
    raise AssertionError(f"timed out waiting for {what}")


@pytest.fixture(scope="module")
def running(sf_dir, platform, tmp_path_factory):
    if shutil.which("docker") is None:
        pytest.skip("docker is not installed")
    named = images_env(sf_dir)
    ensure_images(sf_dir, named)
    folder = tmp_path_factory.mktemp("clock")
    folder.chmod(0o755)
    clock = WorldClock(folder)
    env = {**os.environ, **profile_env(platform, "lean"), **named, "ARENA_CLOCK_DIR": str(folder),
           "PROD_ORDERS_ROWS": "2000"}
    try:
        up = compose(platform, env, "up", "-d", "--pull", "never", *SERVICES)
        assert up.returncode == 0, up.stdout[-2000:] + up.stderr[-2000:]
        clock.start()
        probes = f"http://{compose(platform, env, 'port', 'traffic', '9100').stdout.strip()}"
        wait_for(lambda: get(f"{probes}/healthz")["ok"], 300, "the traffic's probes")
        yield {"env": env, "probes": probes, "clock": clock}
    finally:
        clock.stopped = True
        print(compose(platform, env, "logs", "--tail", "20", "traffic").stdout[-3000:])
        compose(platform, env, "down", "-v", "--remove-orphans")


def test_live_users_browse_order_and_check_stock_without_server_errors(running):
    probes = running["probes"]
    stats = wait_for(lambda: (s := get(f"{probes}/stats"))["journeys"]["completed"] >= 60 and
                     s["by_journey"].get("order", {}).get("requests", 0) >= 5 and s, 240, "60 journeys")
    print("stats", json.dumps(stats))
    assert stats["clock"] == "file" and stats["errors"] == 0 and stats["journeys"]["failed"] == 0
    assert set(stats["by_journey"]) == {"browse", "order", "check-stock"}


def test_live_every_contract_holds_against_production(running):
    doc = wait_for(lambda: (d := get(f"{running['probes']}/contracts")) and
                   all(c["passed"] is not None for c in d.values()) and d, 120, "a contract run")
    assert doc and {k: (c["passed"], c["failures"]) for k, c in doc.items()} == {
        "storefront-inventory": (True, []), "storefront-orders": (True, []), "orders-inventory": (True, [])}


def test_live_the_slo_probe_reads_production_at_world_time(running):
    world_s = (EPOCH + running["clock"].clock_ms) // 1000
    slo = get(f"{running['probes']}/slo?time={world_s}")
    print("slo", json.dumps(slo))
    assert slo["requests_30m"] > 0 and slo["error_rate_30m"] == 0.0 and slo["meets_objective"] is True


def test_live_the_collector_takes_the_traffics_otlp(running):
    sent = wait_for(lambda: (t := get(f"{running['probes']}/stats")["telemetry"])["sent"] >= 3 and t, 60, "exports")
    assert sent["failed"] == 0 and sent["dropped"] == 0


def test_live_a_paused_clock_stops_the_traffic(running):   # Review Focus 3
    probes, clock = running["probes"], running["clock"]
    clock.running.clear()
    try:
        time.sleep(3)                  # past the 1 s lead and any journey still in flight
        before = get(f"{probes}/stats")["requests"]
        time.sleep(6)
        assert get(f"{probes}/stats")["requests"] == before
    finally:
        clock.running.set()
    wait_for(lambda: get(f"{probes}/stats")["requests"] > before, 30, "traffic after the pause")


def test_live_the_traffic_stays_under_its_memory_limit(platform, running):
    name = compose(platform, running["env"], "ps", "-q", "traffic").stdout.strip()
    usage = subprocess.run(["docker", "stats", "--no-stream", "--format", "{{.MemUsage}}", name],
                           capture_output=True, text=True).stdout.strip()
    print("traffic memory", usage)
    assert mib(usage.split("/")[0]) < 64


def mib(text: str) -> float:
    """`docker stats`' `12.3MiB` (or KiB, GiB) in MiB."""
    text = text.strip()
    return float(text[:-3]) * {"KiB": 1 / 1024, "MiB": 1, "GiB": 1024}[text[-3:]]
