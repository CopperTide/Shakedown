"""A tiny HTTP client for the twin tests: one request, the answer read whole."""
import json
import urllib.error
import urllib.request
from dataclasses import dataclass


@dataclass
class Answer:
    status: int
    headers: dict
    body: bytes

    def json(self):
        return json.loads(self.body)


def http(upstream, method, path, body=None, headers=None) -> Answer:
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(upstream + path, data=data, method=method, headers=dict(headers or {}))
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return Answer(r.status, {k.lower(): v for k, v in r.headers.items()}, r.read())
    except urllib.error.HTTPError as e:
        return Answer(e.code, {k.lower(): v for k, v in e.headers.items()}, e.read())
