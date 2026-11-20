"""One HTTP server per twin instance on 127.0.0.1: each request is read whole, handled under the
instance's lock in one store transaction, and answered; a request that fails changes nothing."""
import sys
import threading
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit

from ..forwarded import agent_of, public_base, readonly_of
from .router import Headers, Request, Response

EFFECT_HEADER = "X-Arena-Effect"
READ_METHODS = ("GET", "HEAD", "OPTIONS")


class _Failed(Exception):
    """A 5xx answer: the transaction is rolled back and the answer sent as it is."""

    def __init__(self, response: Response):
        self.response = response


def _read_chunked(rfile) -> bytes:
    out = bytearray()
    while True:
        size = int(rfile.readline().split(b";")[0].strip() or b"0", 16)
        if size == 0:
            while rfile.readline().strip():
                pass
            return bytes(out)
        out += rfile.read(size)
        rfile.readline()


def _read_body(handler) -> bytes:
    if handler.headers.get("Transfer-Encoding", "").lower() == "chunked":
        return _read_chunked(handler.rfile)
    n = int(handler.headers.get("Content-Length") or 0)
    return handler.rfile.read(n) if n else b""


def _request(handler, fallback: str) -> Request:
    url = urlsplit(handler.path)
    headers = Headers(handler.headers.items())
    return Request(handler.command, url.path, parse_qs(url.query, keep_blank_values=True), headers,
                   _read_body(handler), agent_of(headers), public_base(headers, fallback),
                   readonly=readonly_of(headers))


def _handle(twin_server, req: Request) -> Response:
    """Dispatch in one transaction; roll back (store, queued notices, outside work) on a raise or a 5xx."""
    ctx = twin_server.ctx
    with ctx.lock:
        try:
            with ctx.atomic():
                resp = twin_server.router.dispatch(req, ctx)
                if resp.status >= 500:
                    raise _Failed(resp)
        except _Failed as f:
            return f.response
        except Exception:
            traceback.print_exc(file=sys.stderr)
            return Response(500, {"message": "Internal Server Error"})
        effect = resp.effect or ("read" if req.method in READ_METHODS else "write")
        if effect == "write" and resp.status < 400:
            ctx.changed = True
        return resp


class _Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _serve(self):
        req = _request(self, self.server.fallback)
        self._send(_handle(self.server, req))

    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = do_HEAD = do_OPTIONS = _serve

    def _send(self, resp: Response):
        body, ctype = resp.encoded()
        self.send_response(resp.status)
        headers = dict(resp.headers or {})
        if ctype and not any(k.lower() == "content-type" for k in headers):
            headers["Content-Type"] = ctype
        if resp.effect:
            headers[EFFECT_HEADER] = resp.effect
        for k, v in headers.items():
            self.send_header(k, v)
        if resp.stream is None:
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(body)
            return
        self._send_chunks(resp.stream)

    def _send_chunks(self, stream):
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()
        for chunk in stream:
            if chunk:
                self.wfile.write(b"%x\r\n%s\r\n" % (len(chunk), chunk))
        self.wfile.write(b"0\r\n\r\n")

    def log_message(self, format, *args):  # quiet: shaked records every request itself
        pass


class TwinServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, router, ctx):
        super().__init__(("127.0.0.1", 0), _Handler)
        self.router, self.ctx = router, ctx
        self.fallback = f"http://127.0.0.1:{self.server_address[1]}"
        self._thread = threading.Thread(target=self.serve_forever, args=(0.05,), name="twin-http", daemon=True)

    @property
    def upstream(self) -> str:
        return self.fallback

    def start(self) -> "TwinServer":
        self._thread.start()
        return self

    def close(self) -> None:
        self.shutdown()
        self.server_close()
        self._thread.join(timeout=5)
