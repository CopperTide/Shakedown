import hashlib
import hmac
import json
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer

import pytest

from conftest import api, api_raw, commit, push

R = "/api/v3/repos/acme/app"


class Receiver(HTTPServer):
    def __init__(self):
        super().__init__(("127.0.0.1", 0), _Handler)
        self.got = []
        threading.Thread(target=self.serve_forever, args=(0.05,), daemon=True).start()

    @property
    def url(self):
        return f"http://127.0.0.1:{self.server_address[1]}/hook"


class _Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        self.server.got.append(({k.lower(): v for k, v in self.headers.items()}, body))
        self.send_response(200)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def log_message(self, *a):
        pass


@pytest.fixture
def local_receiver():
    r = Receiver()
    yield r
    r.shutdown()
    r.server_close()


def wait_for(pred, timeout=10):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if pred():
            return True
        time.sleep(0.05)
    return False


def make_hook(twin, url, secret=None, events=("push", "pull_request")):
    cfg = {"url": url, "content_type": "json", **({"secret": secret} if secret else {})}
    r = api_raw(twin, "POST", f"{R}/hooks", {"name": "web", "active": True, "events": list(events), "config": cfg})
    assert r.status == 201, r.body
    return r.json()


def test_hook_delivery_signs_the_body(twin, local_receiver):
    hook = make_hook(twin, local_receiver.url, secret="s3cret")
    assert hook["config"]["secret"] == "********"
    assert wait_for(lambda: len(local_receiver.got) == 1)
    headers, body = local_receiver.got[0]
    assert headers["x-github-event"] == "ping" and headers["x-github-hook-id"] == str(hook["id"])
    want = "sha256=" + hmac.new(b"s3cret", body, hashlib.sha256).hexdigest()
    assert headers["x-hub-signature-256"] == want
    assert headers["user-agent"].startswith("GitHub-Hookshot/")
    assert json.loads(body)["hook_id"] == hook["id"]


def test_a_push_is_delivered_after_the_response(twin, clone, local_receiver):
    make_hook(twin, local_receiver.url)
    assert wait_for(lambda: len(local_receiver.got) == 1)
    sha = commit(clone, "b.txt", "two")
    push(clone, twin.token, "HEAD:main")
    assert wait_for(lambda: len(local_receiver.got) == 2)
    headers, body = local_receiver.got[1]
    payload = json.loads(body)
    assert headers["x-github-event"] == "push"
    assert (payload["ref"], payload["after"], payload["repository"]["full_name"]) == ("refs/heads/main", sha, "acme/app")
    assert wait_for(lambda: twin.docs()["hook/1"] == {"deliveries": 2, "failed": 0})


def test_a_hook_to_an_outside_url_is_recorded_failed(twin):
    make_hook(twin, "https://hooks.example.test/x")
    assert wait_for(lambda: twin.docs()["hook/1"] == {"deliveries": 1, "failed": 1})
    hook = api(twin, "GET", f"{R}/hooks/1")
    assert hook["last_response"]["message"] == "outside the world"


def test_hooks_are_listed_edited_pinged_and_deleted(twin, local_receiver):
    make_hook(twin, local_receiver.url)
    assert [h["id"] for h in api(twin, "GET", f"{R}/hooks")] == [1]
    assert api(twin, "PATCH", f"{R}/hooks/1", {"active": False})["active"] is False
    api(twin, "PATCH", f"{R}/hooks/1", {"active": True})
    assert api_raw(twin, "POST", f"{R}/hooks/1/pings").status == 204
    assert wait_for(lambda: len(local_receiver.got) == 2)
    assert api_raw(twin, "DELETE", f"{R}/hooks/1").status == 204
    assert api_raw(twin, "GET", f"{R}/hooks/1").status == 404
