import gzip
import hashlib
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from shakedown_part.twin.vendor_schema import (Pinned, Unavailable, component_schema, documented_ops, fetch,
                                           response_schema)

SPEC = {"openapi": "3.0.1", "paths": {
    "/thing/{id}": {"parameters": [], "get": {"responses": {
        "200": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/Thing"}}}},
        "404": {"$ref": "#/components/responses/NotFound"}}},
        "delete": {"responses": {"204": {"description": "gone"}}},
        "put": {"responses": {"200": {"content": {"application/json": {"example": "{}"}}}}}}},
    "components": {"responses": {"NotFound": {"content": {"application/json": {"schema": {"type": "object"}}}}},
                   "schemas": {"Thing": {"type": "object", "description": "a thing", "properties": {
                       "name": {"type": "string", "nullable": True, "example": "x"},
                       "kind": {"oneOf": [{"$ref": "#/components/schemas/Kind"}, {"type": "integer"}]}}},
                       "Kind": {"type": "string", "enum": ["a", "b"], "nullable": True}}}}
SWAGGER = {"swagger": "2.0", "paths": {"/m.x": {"get": {"responses": {"200": {"schema": {
    "$ref": "#/definitions/ok"}}}}}}, "definitions": {"ok": {"type": "object", "x-extra": 1,
                                                              "properties": {"ok": {"type": "boolean"}}}}}


class _Serve(BaseHTTPRequestHandler):
    def do_GET(self):
        body = self.server.bodies.get(self.path)
        if body is None:
            self.send_response(404)
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


@pytest.fixture
def site():
    server = ThreadingHTTPServer(("127.0.0.1", 0), _Serve)
    server.bodies = {}
    t = threading.Thread(target=server.serve_forever, daemon=True)
    t.start()
    yield server
    server.shutdown()
    server.server_close()


def _url(site, path):
    return f"http://127.0.0.1:{site.server_address[1]}{path}"


def test_fetch_downloads_checks_and_caches(site, tmp_path):
    raw = json.dumps(SPEC).encode()
    site.bodies["/spec.json"] = gzip.compress(raw)   # an archive may answer the original bytes gzipped
    pin = Pinned("spec.json", (_url(site, "/missing"), _url(site, "/spec.json")), hashlib.sha256(raw).hexdigest())
    assert fetch(pin, tmp_path) == raw
    site.bodies.clear()
    assert fetch(pin, tmp_path) == raw   # from the cache now


def test_fetch_refuses_other_bytes_and_says_why(site, tmp_path):
    site.bodies["/spec.json"] = b"{}"
    pin = Pinned("spec.json", (_url(site, "/spec.json"),), "0" * 64)
    with pytest.raises(Unavailable, match="sha256"):
        fetch(pin, tmp_path)
    assert not (tmp_path / "spec.json").exists()


def test_fetch_offline_is_unavailable(tmp_path):
    pin = Pinned("spec.json", ("http://127.0.0.1:9/spec.json",), "0" * 64)
    with pytest.raises(Unavailable, match="spec.json"):
        fetch(pin, tmp_path)


def test_openapi_answers_become_draft7_with_their_definitions():
    s = response_schema(SPEC, "GET", "/thing/{id}", 200)
    assert s["$ref"] == "#/definitions/Thing"
    thing = s["definitions"]["Thing"]
    assert thing["properties"]["name"] == {"type": ["string", "null"]}
    assert thing["properties"]["kind"] == {"anyOf": [{"$ref": "#/definitions/Kind"}, {"type": "integer"}]}
    assert s["definitions"]["Kind"] == {"type": ["string", "null"], "enum": ["a", "b", None]}
    assert response_schema(SPEC, "GET", "/thing/{id}", 404)["type"] == "object"
    assert response_schema(SPEC, "DELETE", "/thing/{id}", 204) == {"type": "null"}
    assert response_schema(SPEC, "GET", "/thing/{id}", 500) is None
    assert response_schema(SPEC, "PUT", "/thing/{id}", 200) == {}   # a body shown only by an example
    assert component_schema(SPEC, "Thing")["definitions"]["Kind"]["enum"] == ["a", "b", None]


def test_a_ref_into_a_component_keeps_its_pointer_and_brings_the_component():
    spec = {"openapi": "3.0.2", "paths": {"/u": {"get": {"responses": {"200": {"content": {"application/json": {
        "schema": {"$ref": "#/components/schemas/User"}}}}}}}},
        "components": {"schemas": {"Tag": {"allOf": [{"type": "object", "properties": {"id": {"type": "string"}}}]},
                                   "User": {"allOf": [{"$ref": "#/components/schemas/Tag/allOf/0"}]}}}}
    s = response_schema(spec, "GET", "/u", 200)
    assert s["definitions"]["User"] == {"allOf": [{"$ref": "#/definitions/Tag/allOf/0"}]}
    assert set(s["definitions"]) == {"User", "Tag"}


def test_swagger2_answers_and_documented_ops():
    s = response_schema(SWAGGER, "GET", "/m.x", 200)
    assert s == {"$ref": "#/definitions/ok", "definitions": {"ok": {"type": "object", "properties": {
        "ok": {"type": "boolean"}}}}}
    assert documented_ops(SPEC) == {("GET", "/thing/{id}"), ("DELETE", "/thing/{id}"), ("PUT", "/thing/{id}")}
