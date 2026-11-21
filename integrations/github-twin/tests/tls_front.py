"""A TLS front for the twin on 127.0.0.1, standing in for shaked's proxy in client tests: a test CA,
and per agent one HTTPS listener that forwards each request to the twin with `X-Forwarded-Proto`,
`X-Forwarded-Host` and that agent's `X-Shakedown-Agent` (a client-sent one is dropped), as the proxy does.
It records the GraphQL requests it forwards."""
import http.client
import json
import os
import ssl
import subprocess
import threading
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

HOP = {"connection", "keep-alive", "transfer-encoding", "content-length", "x-shakedown-agent", "x-shakedown-readonly",
       "x-forwarded-proto", "x-forwarded-host", "host"}


def make_ca(folder: Path) -> tuple[Path, Path, Path]:
    """A CA and a server certificate for 127.0.0.1 and localhost: (ca.pem, server.pem, server.key)."""
    folder.mkdir(parents=True, exist_ok=True)
    ca, ca_key, cert, key, csr = (folder / n for n in ("ca.pem", "ca.key", "server.pem", "server.key", "server.csr"))
    ext = folder / "san.ext"
    ext.write_text("subjectAltName=IP:127.0.0.1,DNS:localhost\nbasicConstraints=critical,CA:FALSE\n"
                   "keyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n"
                   "subjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid,issuer\n")

    def openssl(*args):
        subprocess.run(["openssl", *args], check=True, capture_output=True, timeout=60)

    openssl("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "2", "-subj", "/CN=twin test CA",
            "-addext", "basicConstraints=critical,CA:TRUE", "-addext", "keyUsage=critical,keyCertSign,cRLSign",
            "-keyout", str(ca_key), "-out", str(ca))
    openssl("req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=127.0.0.1", "-keyout", str(key), "-out", str(csr))
    openssl("x509", "-req", "-in", str(csr), "-CA", str(ca), "-CAkey", str(ca_key), "-CAcreateserial", "-days", "2",
            "-extfile", str(ext), "-out", str(cert))
    return ca, cert, key


@dataclass
class Seen:
    agent: str
    operation: str | None
    query: str


class _Forward(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _serve(self):
        front = self.server.front
        n = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(n) if n else None
        front.note(self.server.agent, self.path, body)
        headers = {k: v for k, v in self.headers.items() if k.lower() not in HOP}
        headers.update({"X-Forwarded-Proto": "https", "X-Forwarded-Host": self.headers.get("Host", ""),
                        "X-Shakedown-Agent": self.server.agent, "Content-Length": str(len(body or b""))})
        if self.server.readonly:
            headers["X-Shakedown-Readonly"] = "1"
        up = http.client.HTTPConnection(front.upstream.hostname, front.upstream.port, timeout=60)
        try:
            up.request(self.command, self.path, body=body, headers=headers)
            resp = up.getresponse()
            data = resp.read()
            self.send_response(resp.status)
            for k, v in resp.getheaders():
                if k.lower() not in HOP:
                    self.send_header(k, v)
        finally:
            up.close()
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(data)

    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = do_HEAD = _serve

    def log_message(self, format, *args):
        pass


@dataclass
class TlsFront:
    upstream_url: str
    cert: Path
    key: Path
    seen: list = field(default_factory=list)

    def __post_init__(self):
        self.upstream = urlsplit(self.upstream_url)
        self._servers: dict[str, ThreadingHTTPServer] = {}
        self._lock = threading.Lock()

    def note(self, agent: str, path: str, body: bytes | None) -> None:
        if path != "/api/graphql" or not body:
            return
        b = json.loads(body)
        with self._lock:
            self.seen.append(Seen(agent, b.get("operationName"), b.get("query", "")))

    def listen(self, agent: str, readonly: bool = False) -> str:
        """An HTTPS listener for `agent` (marked read-only, as the proxy marks a read-only agent's
        requests to a part that honours it); its `127.0.0.1:<port>` host."""
        server = ThreadingHTTPServer(("127.0.0.1", 0), _Forward)
        server.daemon_threads = True
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(self.cert, self.key)
        server.socket = ctx.wrap_socket(server.socket, server_side=True)
        server.front, server.agent, server.readonly = self, agent, readonly
        threading.Thread(target=server.serve_forever, args=(0.05,), daemon=True).start()
        self._servers[agent] = server
        return f"127.0.0.1:{server.server_address[1]}"

    def close(self) -> None:
        for server in self._servers.values():
            server.shutdown()
            server.server_close()


def client_env(home: Path, host: str, ca: Path, token: str, agent: str) -> dict:
    """What gh and git need to reach the twin at `host` as `agent`, and nothing from this machine's
    own gh or git setup (GHES mode: `GH_HOST`, `GH_ENTERPRISE_TOKEN`; the test CA)."""
    home.mkdir(parents=True, exist_ok=True)
    return {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": str(home), "GH_CONFIG_DIR": str(home / "gh"),
            "XDG_CONFIG_HOME": str(home / "config"), "XDG_CACHE_HOME": str(home / "cache"), "GH_HOST": host,
            "GH_ENTERPRISE_TOKEN": token, "SSL_CERT_FILE": str(ca), "GIT_SSL_CAINFO": str(ca),
            "GH_NO_UPDATE_NOTIFIER": "1", "GH_NO_EXTENSION_UPDATE_NOTIFIER": "1", "GH_PROMPT_DISABLED": "1",
            "GH_TELEMETRY": "0", "NO_COLOR": "1", "GIT_TERMINAL_PROMPT": "0", "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_AUTHOR_NAME": agent, "GIT_AUTHOR_EMAIL": f"{agent}@acme.test",
            "GIT_COMMITTER_NAME": agent, "GIT_COMMITTER_EMAIL": f"{agent}@acme.test", "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": "credential.helper",
            "GIT_CONFIG_VALUE_0": "!f() { echo username=x-access-token; echo password=$GH_ENTERPRISE_TOKEN; }; f"}
