"""The workbench for real: Docker, the image, the bridge. Skipped without them."""

import shutil
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest

from factory_designs.bridge import ProxyBridge
from factory_designs.workbench import RealDocker, Workbench


def _image_present(tag: str) -> bool:
    if not shutil.which("docker"):
        return False
    return subprocess.run(["docker", "image", "inspect", tag], capture_output=True).returncode == 0


class _Proxy(BaseHTTPRequestHandler):
    """Stands in for the recording proxy: answers every proxied GET with who asked for what."""

    def do_GET(self):  # noqa: N802
        body = f"via proxy: {self.path} auth={self.headers.get('proxy-authorization')}".encode()
        self.send_response(200)
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


@pytest.mark.live
def test_live_commands_reach_only_the_proxy(settings):
    if not _image_present(settings.workbench_image):
        pytest.skip(f"no Docker or no {settings.workbench_image} (workbench/build.sh)")
    proxy = ThreadingHTTPServer(("127.0.0.1", 0), _Proxy)
    threading.Thread(target=proxy.serve_forever, daemon=True).start()
    url = f"http://arena:t1@127.0.0.1:{proxy.server_address[1]}"
    with tempfile.TemporaryDirectory() as d:
        sock, root = Path(d) / "p.sock", Path(d) / "ws"
        try:
            with ProxyBridge(url, sock):
                env = {"HTTP_PROXY": url, "http_proxy": url}  # curl reads only the lower-case one
                wb = Workbench("a", env, root, sock, settings, RealDocker())
                out = wb.shell("curl -s http://code.acme.test/repos")
                assert out.startswith("exit 0\nvia proxy: http://code.acme.test/repos auth=Basic ")
                direct = wb.shell("curl -s -m 5 --noproxy '*' http://1.1.1.1/ || echo no-route")
                assert direct.rstrip().endswith("no-route")
                assert wb.write_file("x.txt", "hi").startswith("wrote")
                assert wb.shell("cat x.txt") == "exit 0\nhi"
                assert wb.shell("touch /etc/nope 2>/dev/null || echo read-only") == "exit 0\nread-only\n"
        finally:
            proxy.shutdown()
            proxy.server_close()
