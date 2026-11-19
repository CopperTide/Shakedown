"""Vendor API descriptions for a twin's conformance tests: fetched once into a cache, pinned by sha256,
and the schema of each documented answer as JSON Schema draft 7.

A description whose licence does not let it be committed is named by a `Pinned` (its cache file,
the URLs to try in order, the sha256 of its bytes); `fetch` raises `Unavailable` with the reason
when no URL gives those bytes, so a test can skip and say why. OpenAPI 3 and Swagger 2 both work:
`nullable` becomes a null type, and `oneOf` becomes `anyOf` (vendor alternatives overlap, so real
answers often match more than one)."""
import gzip
import hashlib
import json
import urllib.request
from dataclasses import dataclass
from pathlib import Path

METHODS = ("get", "put", "post", "patch", "delete")
DROP = {"description", "example", "examples", "discriminator", "readOnly", "writeOnly", "deprecated",
        "externalDocs", "xml", "title"}
TIMEOUT_S = 60


@dataclass(frozen=True)
class Pinned:
    name: str
    urls: tuple
    sha256: str


class Unavailable(Exception):
    """No URL gave the pinned bytes (offline, moved, or changed)."""


def _download(url: str) -> bytes:
    with urllib.request.urlopen(url, timeout=TIMEOUT_S) as r:
        raw = r.read()
    return gzip.decompress(raw) if raw[:2] == b"\x1f\x8b" else raw


def fetch(pinned: Pinned, cache_dir: Path) -> bytes:
    """The pinned bytes, from `cache_dir` when there, else downloaded and cached."""
    path = Path(cache_dir) / pinned.name
    if path.is_file() and hashlib.sha256(path.read_bytes()).hexdigest() == pinned.sha256:
        return path.read_bytes()
    reasons = []
    for url in pinned.urls:
        try:
            raw = _download(url)
        except OSError as e:
            reasons.append(f"{url}: {e}")
            continue
        got = hashlib.sha256(raw).hexdigest()
        if got == pinned.sha256:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
            return raw
        reasons.append(f"{url}: sha256 {got}, pinned {pinned.sha256}")
    raise Unavailable(f"{pinned.name} is not cached and no source gave the pinned bytes: " + "; ".join(reasons))


def load(pinned: Pinned, cache_dir: Path) -> dict:
    return json.loads(fetch(pinned, cache_dir))


def documented_ops(spec: dict) -> set:
    """Every `(METHOD, path)` the description documents."""
    return {(m.upper(), p) for p, item in spec["paths"].items() for m in item if m in METHODS}


def _ref_prefix(spec: dict) -> str:
    return "#/definitions/" if "swagger" in spec else "#/components/schemas/"


def _convert(node, used: set, prefix: str):
    if isinstance(node, list):
        return [_convert(n, used, prefix) for n in node]
    if not isinstance(node, dict):
        return node
    if "$ref" in node:
        used.add(node["$ref"][len(prefix):])
        return {"$ref": "#/definitions/" + node["$ref"][len(prefix):]}
    out = {k: _keyword(k, v, used, prefix) for k, v in node.items()
           if k not in DROP and not k.startswith("x-") and k != "nullable"}
    if "oneOf" in out:
        out["anyOf"] = out.pop("oneOf")
    return _nullable(out) if node.get("nullable") else out


def _keyword(key: str, value, used: set, prefix: str):
    """`properties` maps names (kept as they are, `description` too) to schemas."""
    if key == "properties" and isinstance(value, dict):
        return {name: _convert(v, used, prefix) for name, v in value.items()}
    return _convert(value, used, prefix)


def _nullable(schema: dict) -> dict:
    t = schema.get("type")
    if isinstance(t, str):
        out = {**schema, "type": [t, "null"]}
        if "enum" in out:
            out["enum"] = [*out["enum"], None]
        return out
    return {"anyOf": [schema, {"type": "null"}]}


def _components(spec: dict) -> dict:
    return spec["definitions"] if "swagger" in spec else spec["components"]["schemas"]


def _definitions(spec: dict, used: set, prefix: str) -> dict:
    defs, todo, components = {}, set(used), _components(spec)
    while todo:
        name = todo.pop()
        found: set = set()
        defs[name] = _convert(components[name], found, prefix)
        todo |= found - defs.keys()
    return defs


def _answer_schema(spec: dict, response: dict):
    if "$ref" in response:
        response = spec["components"]["responses"][response["$ref"].split("/")[-1]]
    if "swagger" in spec:
        return response.get("schema")
    content = response.get("content")
    if not content:
        return None
    media = content.get("application/json") or next(iter(content.values()))
    return media.get("schema", {})


def response_schema(spec: dict, method: str, path: str, status: int) -> dict | None:
    """The draft 7 schema of the documented answer: `{"type": "null"}` for one without a body, `{}`
    for a body shown only by an example; None when the description documents no such answer."""
    responses = spec["paths"][path][method.lower()]["responses"]
    response = responses.get(str(status))
    if response is None:
        return None
    schema = _answer_schema(spec, response)
    return {"type": "null"} if schema is None else _with_definitions(spec, schema)


def component_schema(spec: dict, name: str) -> dict:
    """A named component as draft 7, for an answer the description shows only by an example."""
    return _with_definitions(spec, _components(spec)[name])


def _with_definitions(spec: dict, schema: dict) -> dict:
    used, prefix = set(), _ref_prefix(spec)
    out = _convert(schema, used, prefix)
    definitions = _definitions(spec, used, prefix)
    return {**out, "definitions": definitions} if definitions else out
