"""PagerDuty-style ids: `P` and six of A-Z0-9, made from a name so a world is the same every run.
Incidents are `P<number>` (their incident number), as the world's checks name them."""
import base64
import hashlib


def make(kind: str, name: str) -> str:
    digest = hashlib.sha256(f"pagerduty:{kind}:{name}".encode()).digest()
    return "P" + base64.b32encode(digest).decode()[:6]


def incident(number: int) -> str:
    return f"P{number}"


def incident_number(incident_id: str) -> int | None:
    rest = str(incident_id or "")[1:]
    return int(rest) if str(incident_id).startswith("P") and rest.isdigit() else None
