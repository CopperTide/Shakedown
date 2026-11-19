"""Requests, responses and the router of an HTTP twin: a method and a path pattern to a handler."""
import json
import re
from dataclasses import dataclass, field
from typing import Callable, Iterable, Iterator, Mapping
from urllib.parse import unquote

JSON_TYPE = "application/json; charset=utf-8"


class Headers(Mapping):
    """Header names in any case; the last value of a repeated header wins."""

    def __init__(self, pairs: Iterable[tuple[str, str]] = ()):
        self._d = {k.lower(): v for k, v in pairs}

    def __getitem__(self, name: str) -> str:
        return self._d[name.lower()]

    def __iter__(self) -> Iterator[str]:
        return iter(self._d)

    def __len__(self) -> int:
        return len(self._d)


@dataclass
class Request:
    method: str
    path: str
    query: dict[str, list[str]]
    headers: Mapping[str, str]
    body: bytes
    agent: str | None
    base: str
    params: dict[str, str] = field(default_factory=dict)

    def json(self):
        """The body as JSON; an empty body is `{}`."""
        return json.loads(self.body) if self.body.strip() else {}

    def arg(self, name: str, default: str | None = None) -> str | None:
        """The first value of a query parameter."""
        return self.query.get(name, [default])[0]


@dataclass
class Response:
    status: int = 200
    body: object = None
    headers: dict | None = None
    effect: str | None = None
    stream: Iterable[bytes] | None = None

    def encoded(self) -> tuple[bytes, str | None]:
        """The body as bytes and its default content type: dict/list → JSON, str → text, bytes as is."""
        if self.body is None:
            return b"", None
        if isinstance(self.body, (bytes, bytearray)):
            return bytes(self.body), None
        if isinstance(self.body, str):
            return self.body.encode(), "text/plain; charset=utf-8"
        return json.dumps(self.body).encode(), JSON_TYPE


Handler = Callable[[Request, object], Response]


def _compile(pattern: str) -> re.Pattern:
    """`{name}` matches one segment, `{name:path}` the rest of the path (maybe empty)."""
    out, pos = [], 0
    for m in re.finditer(r"\{(\w+)(:path)?\}", pattern):
        out.append(re.escape(pattern[pos:m.start()]))
        out.append(f"(?P<{m.group(1)}>.*)" if m.group(2) else f"(?P<{m.group(1)}>[^/]+)")
        pos = m.end()
    out.append(re.escape(pattern[pos:]))
    return re.compile("".join(out))


@dataclass
class Route:
    method: str
    pattern: str
    regex: re.Pattern
    handler: Handler


def not_found(req: Request) -> Response:
    return Response(404, {"message": "Not Found"})


class Router:
    """Routes in the order they were added; the first whose method and pattern match handles."""

    def __init__(self):
        self.routes: list[Route] = []

    def add(self, method: str, pattern: str, handler: Handler) -> None:
        self.routes.append(Route(method.upper(), pattern, _compile(pattern), handler))

    def route(self, method: str, pattern: str):
        def register(fn: Handler) -> Handler:
            self.add(method, pattern, fn)
            return fn

        return register

    def not_found(self, req: Request) -> Response:
        return not_found(req)

    def dispatch(self, req: Request, ctx) -> Response:
        for r in self.routes:
            m = r.regex.fullmatch(req.path)
            if m is not None and r.method == req.method:
                req.params = {k: unquote(v) for k, v in m.groupdict().items()}
                return r.handler(req, ctx)
        return self.not_found(req)
