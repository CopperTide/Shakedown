"""`python -m traffic`: load the settings, serve the probes, tick on the world clock every `tick_ms` and export
telemetry every `export_every_s` until SIGTERM."""
import os
import signal
import sys
import threading
import traceback
from pathlib import Path

from .clock import Clock
from .config import load
from .otlp import Telemetry
from .runner import Traffic
from .server import start

DATA = Path(__file__).resolve().parent / "journeys.json"


def export_every(telemetry: Telemetry, seconds: float, stop: threading.Event) -> None:
    while not stop.wait(seconds):
        telemetry.export()


def tick_until(traffic: Traffic, seconds: float, stop: threading.Event) -> None:
    while not stop.wait(seconds):
        try:
            traffic.tick()
        except Exception:  # a bad tick is logged; the next one tries again
            traceback.print_exc(file=sys.stderr)


def main(env=os.environ) -> int:
    config = load(Path(env.get("TRAFFIC_CONFIG", DATA)), env)
    telemetry = Telemetry({"service.name": "traffic", "service.version": "1",
                           "deployment.environment.name": config.environment}, config.otlp_endpoint)
    traffic = Traffic(config, Clock(config.clock_file), telemetry)
    server, stop = start(traffic, config.port), threading.Event()
    for sig in (signal.SIGTERM, signal.SIGINT):
        signal.signal(sig, lambda *_: stop.set())
    threading.Thread(target=export_every, args=(telemetry, config.export_every_s, stop), daemon=True).start()
    print(f"traffic: probes on :{config.port}, seed {config.seed}, clock {traffic.clock.source()}", flush=True)
    tick_until(traffic, config.tick_ms / 1000, stop)
    server.shutdown()
    traffic.close()
    telemetry.export()
    return 0


if __name__ == "__main__":
    sys.exit(main())
