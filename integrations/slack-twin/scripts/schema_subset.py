"""Write the Slack Web API extract the schema tests read: the success (`200`) and error (`default`)
answers of the covered methods, as JSON Schema draft 7, with the definitions they use. Slack's
description writes "one of these" as a list-valued `items` on a schema that is not an array; draft 7
reads that as tuple items (so it would check nothing), and the extract makes it `anyOf`.

    uv run python scripts/schema_subset.py <slack_web_openapi_v2_without_examples.json> <url> <commit> \
        tests/schemas/slack-web-subset.json
"""
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shakedown_part.twin.vendor_schema import response_schema  # noqa: E402
from slack_twin.methods import METHODS  # noqa: E402


def one_of_items(node):
    """List-valued `items` on a non-array schema → `anyOf` (recursively)."""
    if isinstance(node, list):
        return [one_of_items(n) for n in node]
    if not isinstance(node, dict):
        return node
    out = {k: one_of_items(v) for k, v in node.items()}
    if isinstance(out.get("items"), list) and out.get("type") != "array":
        out["anyOf"] = out.pop("items")
    return out


def extract(spec: dict) -> tuple[dict, dict]:
    responses, definitions = {}, {}
    for name in sorted(METHODS):
        method = next(m for m in spec["paths"][f"/{name}"] if m in ("get", "post"))
        for status in ("200", "default"):
            schema = one_of_items(response_schema(spec, method, f"/{name}", status))
            definitions.update(schema.pop("definitions", {}))
            responses[f"{name} {status}"] = schema
    return responses, dict(sorted(definitions.items()))


def main(src: str, url: str, commit: str, out: str) -> None:
    raw = Path(src).read_bytes()
    spec = json.loads(raw)
    responses, definitions = extract(spec)
    doc = {"source": {"url": url, "commit": commit, "sha256": hashlib.sha256(raw).hexdigest(),
                      "info_version": spec["info"]["version"]},
           "methods": sorted(p.lstrip("/") for p in spec["paths"]), "responses": responses, "definitions": definitions}
    Path(out).write_text(json.dumps(doc, indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:5])
