"""The probes on port 9100 the platform part reads as docs: `/stats` (totals since start; `?window_s=` adds a
world-time window ending at `?time=` world seconds, else now), `/slo` (production's SLO over the last 30 world
minutes at `?time=`), `/contracts` (the last contract run) and `/healthz`."""
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit


class BadQuery(ValueError):
    pass


def world_ms(query: dict, traffic) -> int:
    raw = query.get("time", [None])[0]
    if raw is None:
        return traffic.clock.now().world_ms
    try:
        return int(float(raw) * 1000)
    except ValueError:
        raise BadQuery(f"time must be world seconds, not {raw!r}") from None


def stats(traffic, query: dict) -> dict:
    doc = traffic.stats_doc()
    if "window_s" in query:
        try:
            length_ms = int(float(query["window_s"][0]) * 1000)
        except ValueError:
            raise BadQuery(f"window_s must be seconds, not {query['window_s'][0]!r}") from None
        doc["window"] = traffic.stats.window(world_ms(query, traffic), length_ms)
    return doc


ROUTES = {
    "/healthz": lambda traffic, query: {"ok": True},
    "/stats": stats,
    "/slo": lambda traffic, query: traffic.stats.slo(world_ms(query, traffic)),
    "/contracts": lambda traffic, query: traffic.contracts_doc(),
}


def handler_for(traffic):
    class Probes(BaseHTTPRequestHandler):
        def do_GET(self):
            url = urlsplit(self.path)
            route = ROUTES.get(url.path)
            try:
                status, doc = (200, route(traffic, parse_qs(url.query))) if route else (404, {"error": "no probe"})
            except BadQuery as e:
                status, doc = 400, {"error": str(e)}
            raw = json.dumps(doc).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)

        def log_message(self, *args):
            pass
    return Probes


def start(traffic, port: int, host: str = "0.0.0.0") -> ThreadingHTTPServer:
    server = ThreadingHTTPServer((host, port), handler_for(traffic))
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, name="probes", daemon=True).start()
    return server
