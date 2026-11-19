"""Every covered operation's answer checked against Atlassian's descriptions (platform v2 and v3, and
Jira Software's agile API), and every route a covered, documented operation."""
import pytest
from jsonschema import Draft7Validator

from arena_part.twin.vendor_schema import component_schema, documented_ops, response_schema
from conftest import config, started
from exercise import exercise
from jira_twin.part import JiraTwin
from jira_twin.rest.covered import COVERED, documented_path, router_ops
from vendor import PLATFORM_V2, PLATFORM_V3, SOFTWARE, spec

SERVER_ONLY = {("platform", "GET", "/search"), ("platform", "POST", "/search")}
# Jira Software documents these sprint answers only by an example; its SprintBean is the shape shown.
SPRINT_ANSWERS = {("POST", "/sprint"), ("GET", "/sprint/{sprintId}"), ("PUT", "/sprint/{sprintId}"),
                  ("POST", "/sprint/{sprintId}")}


def _schema(description: dict, api: str, method: str, path: str, status: int, version: str) -> dict:
    if api == "agile" and (method, path) in SPRINT_ANSWERS and status < 300:
        return component_schema(description, "SprintBean")
    if (api, method, path) == ("agile", "GET", "/board/{boardId}/sprint") and status == 200:
        bean = component_schema(description, "SprintBean")
        return {"type": "object", "required": ["values", "isLast"], "properties": {
            "values": {"type": "array", "items": {k: v for k, v in bean.items() if k != "definitions"}},
            "isLast": {"type": "boolean"}, "startAt": {"type": "integer"}, "maxResults": {"type": "integer"}}}
    schema = response_schema(description, method, documented_path(api, path, version), status)
    assert schema is not None, f"{method} {path} {status} is not a documented answer"
    return schema


@pytest.fixture
def jira_for(request):
    made = []

    def make(op):
        made.append(started(config(deployment="server") if op in SERVER_ONLY else None))
        return made[-1]

    yield make
    for t in made:
        t.part.stop("i")


CASES = [(op, v) for op in COVERED for v in (("2", "3") if op[0] == "platform" else ("1.0",))]


@pytest.mark.parametrize("op,v", CASES, ids=lambda x: " ".join(x) if isinstance(x, tuple) else f"v{x}")
def test_covered_operation_matches_atlassian(jira_for, op, v):
    description = spec(SOFTWARE if op[0] == "agile" else PLATFORM_V3 if v == "3" else PLATFORM_V2)
    status, body = exercise(jira_for(op), op, v)
    assert status < 400, (status, body)
    errors = sorted(Draft7Validator(_schema(description, *op, status, v)).iter_errors(body), key=str)
    assert not errors, "\n".join(f"{list(e.absolute_path)}: {e.message}" for e in errors[:10])


def test_every_route_is_covered_and_documented():
    assert sorted(router_ops(JiraTwin().router())) == sorted(COVERED)
    documented = {"2": documented_ops(spec(PLATFORM_V2)), "3": documented_ops(spec(PLATFORM_V3)),
                  "agile": documented_ops(spec(SOFTWARE))}
    for api, method, path in COVERED:
        versions = ("2", "3") if api == "platform" else ("agile",)
        for v in versions:
            assert (method, documented_path(api, path, v)) in documented[v], (api, method, path, v)
