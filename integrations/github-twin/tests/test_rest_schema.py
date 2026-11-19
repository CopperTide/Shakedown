import json
from pathlib import Path

import pytest
from jsonschema import Draft7Validator

from exercise import exercise
from github_twin.part import GithubTwin
from github_twin.rest.covered import COVERED, router_ops

SUBSET = json.loads((Path(__file__).parent / "schemas" / "ghes-3.20-subset.json").read_text())


def response_schema(subset, op, status):
    key = f"{op[0]} {op[1]} {status}"
    assert key in subset["responses"], f"{key} is not a documented answer"
    return {**subset["responses"][key], "definitions": subset["definitions"]}


@pytest.mark.parametrize("op", COVERED, ids=lambda op: f"{op[0]} {op[1]}")
def test_covered_operation_matches_the_ghes_schema(twin, op, tmp_path):
    status, body = exercise(twin, op, tmp_path)
    errors = sorted(Draft7Validator(response_schema(SUBSET, op, status)).iter_errors(body), key=str)
    assert not errors, "\n".join(f"{list(e.absolute_path)}: {e.message}" for e in errors[:10])


def test_every_route_is_covered_and_documented():
    documented = {tuple(op) for op in SUBSET["paths_ops"]}
    assert set(router_ops(GithubTwin().router())) == set(COVERED) and set(COVERED) <= documented


def test_the_extract_names_its_source():
    assert SUBSET["source"]["sha256"] and SUBSET["source"]["url"].startswith("https://raw.githubusercontent.com/github/rest-api-description/")
