"""Every covered operation's answer checked against PagerDuty's descriptions (REST v2 and Events v2),
and every route a covered, documented operation."""
import pytest
from jsonschema import Draft7Validator

from arena_part.twin.vendor_schema import documented_ops, response_schema
from conftest import enqueue, rest
from pagerduty_twin.covered import COVERED, router_ops
from pagerduty_twin.part import PagerDutyTwin
from vendor import EVENTS, REST, spec

NOTE = {"note": {"content": "Rolling back"}}
ACK = {"incident": {"type": "incident_reference", "status": "acknowledged"}}


def _sample(pd, op):
    """One request per covered operation: `(method, path, body, expected status)`."""
    enqueue(pd, "trigger", "k1")
    rest(pd, "POST", "/incidents/P1/notes", NOTE)
    return {("GET", "/incidents"): ("GET", "/incidents?total=true", None),
            ("GET", "/incidents/{id}"): ("GET", "/incidents/P1", None),
            ("PUT", "/incidents/{id}"): ("PUT", "/incidents/P1", ACK),
            ("GET", "/incidents/{id}/notes"): ("GET", "/incidents/P1/notes", None),
            ("POST", "/incidents/{id}/notes"): ("POST", "/incidents/P1/notes", NOTE),
            ("POST", "/enqueue"): ("POST", "/v2/enqueue", {"routing_key": "0123456789abcdef0123456789abcdef",
                                                          "event_action": "acknowledge", "dedup_key": "k1"}),
            }.get(op[1:], (op[1], op[2], None))


@pytest.mark.parametrize("op", COVERED, ids=lambda op: " ".join(op))
def test_covered_operation_matches_pagerduty(pd, op):
    from conftest import EVENTS_HOST, raw
    method, path, body = _sample(pd, op)
    host = {"host": EVENTS_HOST, "token": None, "sender": None} if op[0] == "events" else {}
    status, _, answer = raw(pd, method, path, body, **host)
    assert status < 400, (status, answer)
    schema = response_schema(spec(EVENTS if op[0] == "events" else REST), op[1], op[2], status)
    assert schema is not None, f"{op} {status} is not a documented answer"
    errors = sorted(Draft7Validator(schema).iter_errors(answer), key=str)
    assert not errors, "\n".join(f"{list(e.absolute_path)}: {e.message}" for e in errors[:10])


def test_every_route_is_covered_and_documented():
    assert sorted(router_ops(PagerDutyTwin().router())) == sorted(COVERED)
    documented = {"rest": documented_ops(spec(REST)), "events": documented_ops(spec(EVENTS))}
    assert all((m, p) in documented[api] for api, m, p in COVERED)
