"""Write the GHES REST extract the schema tests read: every documented operation's name, and the
JSON answers of the covered ones (`METHOD path status` → schema), with the component schemas they
use. OpenAPI 3.0's `nullable` becomes a JSON Schema null type, and `oneOf` becomes `anyOf`: the
description's alternatives overlap (`simple-user` or an open `empty-object`), so no real answer
matches exactly one.

    uv run python scripts/schema_subset.py <ghes-3.20.2022-11-28.json> <url> <commit> tests/schemas/ghes-3.20-subset.json
"""
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from github_twin.rest.covered import COVERED  # noqa: E402

DROP = {"description", "example", "examples", "discriminator", "readOnly", "writeOnly", "deprecated", "externalDocs"}
COMPONENT = "#/components/schemas/"


def convert(node, used: set):
    """A JSON Schema draft 7 copy of an OpenAPI schema node; names the components it refers to."""
    if isinstance(node, list):
        return [convert(n, used) for n in node]
    if not isinstance(node, dict):
        return node
    if "$ref" in node:
        used.add(node["$ref"][len(COMPONENT):])
        return {"$ref": "#/definitions/" + node["$ref"][len(COMPONENT):]}
    out = {k: _keyword(k, v, used) for k, v in node.items()
           if k not in DROP and not k.startswith("x-") and k != "nullable"}
    if "oneOf" in out:
        out["anyOf"] = out.pop("oneOf")
    if node.get("nullable"):
        return _nullable(out)
    return out


def _keyword(key: str, value, used: set):
    """`properties` maps names (kept as they are, `description` too) to schemas."""
    if key == "properties":
        return {name: convert(v, used) for name, v in value.items()}
    return convert(value, used)


def _nullable(schema: dict) -> dict:
    t = schema.get("type")
    if isinstance(t, str):
        out = {**schema, "type": [t, "null"]}
        if "enum" in out:
            out["enum"] = [*out["enum"], None]
        return out
    return {"anyOf": [schema, {"type": "null"}]}


def _answer(spec: dict, response: dict):
    if "$ref" in response:
        response = spec["components"]["responses"][response["$ref"].split("/")[-1]]
    content = response.get("content", {})
    media = content.get("application/json") or next(iter(content.values()), {})
    return media.get("schema")


def _definitions(spec: dict, used: set) -> dict:
    defs, todo = {}, set(used)
    while todo:
        name = todo.pop()
        found: set = set()
        defs[name] = convert(spec["components"]["schemas"][name], found)
        todo |= found - defs.keys()
    return dict(sorted(defs.items()))


def extract(spec: dict) -> tuple[dict, dict]:
    responses, used = {}, set()
    for method, path in COVERED:
        op = spec["paths"][path][method.lower()]
        for status, response in op["responses"].items():
            schema = _answer(spec, response)
            responses[f"{method} {path} {status}"] = convert(schema, used) if schema else {"type": "null"}
    return responses, _definitions(spec, used)


def main(src: str, url: str, commit: str, out: str) -> None:
    raw = Path(src).read_bytes()
    spec = json.loads(raw)
    responses, definitions = extract(spec)
    ops = sorted([m.upper(), p] for p, item in spec["paths"].items() for m in item
                 if m in ("get", "put", "post", "patch", "delete"))
    doc = {"source": {"url": url, "commit": commit, "sha256": hashlib.sha256(raw).hexdigest(),
                      "openapi": spec["openapi"], "info_version": spec["info"]["version"]},
           "paths_ops": ops, "responses": responses, "definitions": definitions}
    Path(out).write_text(json.dumps(doc, indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:5])
