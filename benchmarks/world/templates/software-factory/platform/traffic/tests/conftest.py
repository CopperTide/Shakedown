"""The traffic service's own tests: stdlib and pytest only, as the service runs on plain python:3.12-slim. They
import the service as the package `traffic` from the platform folder."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

import json  # noqa: E402
import threading  # noqa: E402
import time  # noqa: E402
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer  # noqa: E402

import pytest  # noqa: E402


class Route:
    """What a fake server answers on a path: a status, a JSON body (or raw bytes), after a delay."""

    def __init__(self, body=None, status=200, delay_s=0.0):
        self.body, self.status, self.delay_s = body, status, delay_s


def handler_for(routes: dict, seen: list):
    class Handler(BaseHTTPRequestHandler):
        def answer(self):
            length = int(self.headers.get("Content-Length") or 0)
            seen.append({"method": self.command, "path": self.path, "headers": {k.lower(): v for k, v in self.headers.items()},
                         "body": json.loads(self.rfile.read(length)) if length else None})
            route = routes.get(f"{self.command} {self.path}") or routes.get(self.path) or Route({"detail": "no"}, 404)
            time.sleep(route.delay_s)
            raw = route.body if isinstance(route.body, bytes) else json.dumps(route.body).encode()
            self.send_response(route.status)
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)

        do_GET = do_POST = answer

        def log_message(self, *args):
            pass
    return Handler


@pytest.fixture
def serve():
    """`serve(routes) -> (base_url, seen)`: a real HTTP server on 127.0.0.1 answering `routes` (path or
    "METHOD path" → Route or a JSON body), recording each request it was sent."""
    servers = []

    def start(routes: dict):
        seen: list = []
        routes = {k: v if isinstance(v, Route) else Route(v) for k, v in routes.items()}
        server = ThreadingHTTPServer(("127.0.0.1", 0), handler_for(routes, seen))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        servers.append(server)
        return f"http://127.0.0.1:{server.server_address[1]}", seen
    yield start
    for server in servers:
        server.shutdown()
        server.server_close()


@pytest.fixture
def serve_json(serve):
    return lambda routes: serve(routes)[0]


@pytest.fixture
def closed_port():
    import socket
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return f"http://127.0.0.1:{s.getsockname()[1]}"
