import socket
import socketserver
import tempfile
import threading
from pathlib import Path

from factory_designs.bridge import ProxyBridge


class _Echo(socketserver.BaseRequestHandler):
    def handle(self):
        while data := self.request.recv(4096):
            self.request.sendall(data.upper())


def test_bridge_pipes_bytes_both_ways():
    server = socketserver.ThreadingTCPServer(("127.0.0.1", 0), _Echo)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = server.server_address[1]
    with tempfile.TemporaryDirectory() as d:
        sock = Path(d) / "proxy.sock"
        try:
            with ProxyBridge(f"http://shakedown:t1@127.0.0.1:{port}", sock):
                for word in (b"connect github.acme.test:443", b"second"):
                    with socket.socket(socket.AF_UNIX) as c:
                        c.connect(str(sock))
                        c.sendall(word)
                        c.shutdown(socket.SHUT_WR)
                        assert c.recv(4096) == word.upper()
            assert not sock.exists()
        finally:
            server.shutdown()
            server.server_close()
