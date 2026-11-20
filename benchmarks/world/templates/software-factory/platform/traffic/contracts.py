"""The contract probe: each consumer's expectations of its providers (`<dir>/<consumer>/<provider>.json`, the
format of the template's `contracts.schema.json`) run against the providers' production addresses. A call fails
when there is no answer, the status is not 2xx, the body is not JSON, or a field is missing or of another JSON
type; the field lines are worded as the apps' `platform_lib.contracts` words them."""
import json
from pathlib import Path

from .client import request

TIMEOUT_S = 2.0


def json_type(value) -> str:
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, (int, float)):
        return "number"
    return {str: "string", list: "array", dict: "object", type(None): "null"}.get(type(value), "unknown")


def field_failures(call: dict, body) -> list[str]:
    where = f"{call['method']} {call['path']}"
    if not isinstance(body, dict):
        return [f"{where}: the body is {json_type(body)}, expected object"]
    out = []
    for name, expected in call["fields"].items():
        if name not in body:
            out.append(f"{where}: field {name} missing")
        elif json_type(body[name]) != expected:
            out.append(f"{where}: {name} is {json_type(body[name])}, expected {expected}")
    return out


def call_failures(call: dict, base_url: str) -> list[str]:
    where = f"{call['method']} {call['path']}"
    out = request(call["method"], f"{base_url}{call['path']}", timeout_s=TIMEOUT_S)
    if out.status is None:
        return [f"{where}: no answer ({out.error})"]
    if not 200 <= out.status < 300:
        return [f"{where}: status {out.status}, expected 2xx"]
    if not out.json:
        return [f"{where}: the body is not JSON"]
    return field_failures(call, out.body)


def check(contract: dict, base_url: str) -> list[str]:
    """Why production's provider at `base_url` does not meet `contract`, one line each; empty when it does."""
    return [line for call in contract["calls"] for line in call_failures(call, base_url)]


def load(folder: Path) -> list[dict]:
    return [json.loads(p.read_text(encoding="utf-8")) for p in sorted(Path(folder).glob("*/*.json"))]


def run_all(contracts: list[dict], services: dict, at_world_ms: int) -> dict:
    """`{"<consumer>-<provider>": {passed, failures, at_world_ms}}` for every contract."""
    doc = {}
    for c in contracts:
        base = services.get(c["provider"])
        failures = check(c, base) if base else [f"no address for the provider {c['provider']}"]
        doc[f"{c['consumer']}-{c['provider']}"] = {"passed": not failures, "failures": failures,
                                                   "at_world_ms": at_world_ms}
    return doc
