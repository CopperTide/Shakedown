"""`python -m traffic` as the platform runs it: data file, clock file, contracts folder and port from the
environment, probes answering while it runs, and stopping on SIGTERM."""
import json
import os
import signal
import socket
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

from test_runner import CONTRACT, EPOCH, SHOP, data

PLATFORM = Path(__file__).resolve().parents[2]


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def get(url: str):
    with urllib.request.urlopen(url, timeout=2) as r:
        return json.loads(r.read())


def wait_for(fn, seconds=10.0):
    deadline = time.monotonic() + seconds
    while True:
        try:
            value = fn()
            if value:
                return value
        except OSError:
            pass
        assert time.monotonic() < deadline, "timed out"
        time.sleep(0.1)


def test_the_service_runs_on_the_clock_file_and_stops_on_sigterm(serve, tmp_path):
    base, seen = serve(SHOP)
    (tmp_path / "journeys.json").write_text(json.dumps(data(base, journeys={**data(base)["journeys"]})))
    (tmp_path / "contracts" / "web").mkdir(parents=True)
    (tmp_path / "contracts" / "web" / "shop.json").write_text(json.dumps(CONTRACT))
    clock = tmp_path / "clock.json"
    wall = int(time.time() * 1000)
    clock.write_text(json.dumps({"epoch_ms": EPOCH, "clock_ms": 0, "world_ms": EPOCH, "speed": 1.0, "wall_ms": wall}))
    port = free_port()
    env = {**os.environ, "TRAFFIC_CONFIG": str(tmp_path / "journeys.json"), "SHAKEDOWN_CLOCK_FILE": str(clock),
           "TRAFFIC_CONTRACTS": str(tmp_path / "contracts"), "TRAFFIC_PORT": str(port), "PYTHONPATH": str(PLATFORM)}
    proc = subprocess.Popen([sys.executable, "-m", "traffic"], env=env, cwd=tmp_path)
    try:
        probes = f"http://127.0.0.1:{port}"
        assert wait_for(lambda: get(f"{probes}/contracts")["web-shop"]["passed"])
        clock.write_text(json.dumps({"epoch_ms": EPOCH, "clock_ms": 600_000, "world_ms": EPOCH + 600_000,
                                     "speed": 1.0, "wall_ms": int(time.time() * 1000)}))
        stats = wait_for(lambda: (s := get(f"{probes}/stats"))["requests"] > 100 and s)
        assert stats["clock"] == "file" and stats["errors"] == 0
        proc.send_signal(signal.SIGTERM)
        assert proc.wait(timeout=10) == 0
    finally:
        proc.kill()
