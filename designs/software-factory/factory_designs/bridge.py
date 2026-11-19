"""The workbench's one way out: a unix socket on this machine, mounted into each container, whose
every connection is piped, both ways, to Shakedown's recording proxy. In the container, the
image's ``bridge-then`` relays ``127.0.0.1:3128`` to the socket, so the agents' clients use the
proxy as usual, with their own ticket in the proxy URL."""

from __future__ import annotations

import socket
import threading
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit


def _pipe(src: socket.socket, dst: socket.socket) -> None:
    try:
        while data := src.recv(65536):
            dst.sendall(data)
    except OSError:
        pass
    finally:
        try:
            dst.shutdown(socket.SHUT_WR)
        except OSError:
            pass


def _serve(client: socket.socket, target: tuple[str, int]) -> None:
    try:
        upstream = socket.create_connection(target, timeout=30)
    except OSError:
        client.close()
        return
    upstream.settimeout(None)
    back = threading.Thread(target=_pipe, args=(upstream, client), daemon=True)
    back.start()
    _pipe(client, upstream)
    back.join()
    client.close()
    upstream.close()


class ProxyBridge:
    """Listens on ``socket`` while open and pipes each connection to ``proxy_url``'s host:port."""

    def __init__(self, proxy_url: str, socket_path: Path):
        u = urlsplit(proxy_url)
        self.target = (u.hostname or "127.0.0.1", u.port or 80)
        self.socket_path = socket_path
        self._server: socket.socket | None = None

    def __enter__(self) -> "ProxyBridge":
        self.socket_path.unlink(missing_ok=True)
        srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        srv.bind(str(self.socket_path))
        self.socket_path.chmod(0o600)
        srv.listen(64)
        self._server = srv
        threading.Thread(target=self._accept, daemon=True).start()
        return self

    def _accept(self) -> None:
        assert self._server is not None
        while True:
            try:
                client, _ = self._server.accept()
            except OSError:
                return  # closed
            threading.Thread(target=_serve, args=(client, self.target), daemon=True).start()

    def __exit__(self, *exc: Any) -> None:
        if self._server is not None:
            try:
                self._server.shutdown(socket.SHUT_RDWR)  # wakes the accepting thread
            except OSError:
                pass
            self._server.close()
        self.socket_path.unlink(missing_ok=True)
