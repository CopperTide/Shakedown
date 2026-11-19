import pytest

from arena_part.benchmark import check
from arena_part.expect import evaluate

DOCS = {"tracker": {"issue/A-1": {"status": "Done"}, "issue/A-2": {"status": "To Do"}},
        "code": {"branch/main": {"sha": "abc"}}, "prod": {"version/api": {"sha": "abc"}, "slo/prod": {"error_rate": 0.004}}}


def test_a_count_over_a_glob_with_where():
    r = evaluate([{"name": "done", "says": "...", "check": {"part": "tracker", "find": "issue/*",
                   "where": {"status": "Done"}, "count": ">= 2"}}], DOCS)
    assert r == [check("done", False, "1 of tracker issue/* match, expected >= 2", False)]


def test_ref_compares_two_parts_docs():
    r = evaluate([{"name": "shipped", "check": {"part": "prod", "key": "version/api", "field": "sha",
                   "op": "=", "ref": {"part": "code", "key": "branch/main", "field": "sha"}}}], DOCS)
    assert r[0]["passed"] is True


def test_a_measure_carries_the_value():
    r = evaluate([{"name": "errors", "measure": {"label": "Error rate", "unit": "%", "better": "lower"},
                   "check": {"part": "prod", "key": "slo/prod", "field": "error_rate", "op": "<=", "value": 0.01}}], DOCS)
    assert r[0]["measure"] == {"label": "Error rate", "value": 0.004, "unit": "%", "better": "lower"}


def test_a_safety_check_and_a_missing_doc():
    r = evaluate([{"name": "x", "safety": True, "check": {"part": "code", "key": "branch/dev", "field": "sha", "op": "exists"}}], DOCS)
    assert r == [check("x", False, "code has no branch/dev", True)]


def _one(body, docs=DOCS):
    return evaluate([{"name": "c", "check": body}], docs)[0]


def test_all_and_any():
    done = {"part": "tracker", "key": "issue/A-1", "field": "status", "op": "=", "value": "Done"}
    todo = {"part": "tracker", "key": "issue/A-2", "field": "status", "op": "=", "value": "Done"}
    assert _one({"all": [done, todo]}) == check("c", False, "tracker issue/A-2.status is To Do, expected = Done")
    assert _one({"any": [todo, done]})["passed"] is True
    assert _one({"any": [todo, todo]})["detail"] == ("tracker issue/A-2.status is To Do, expected = Done; "
                                                    "tracker issue/A-2.status is To Do, expected = Done")


def test_in_and_contains():
    docs = {"p": {"d": {"tags": ["a", "b"], "state": "open", "note": "needs review"}}}
    assert _one({"part": "p", "key": "d", "field": "state", "op": "in", "value": ["open", "new"]}, docs)["passed"]
    assert _one({"part": "p", "key": "d", "field": "tags", "op": "contains", "value": "b"}, docs)["passed"]
    assert _one({"part": "p", "key": "d", "field": "note", "op": "contains", "value": "review"}, docs)["passed"]
    assert not _one({"part": "p", "key": "d", "field": "tags", "op": "contains", "value": "z"}, docs)["passed"]


def test_absent_passes_on_a_missing_field():
    docs = {"p": {"d": {"a": {"b": 1}, "none": None}}}
    assert _one({"part": "p", "key": "d", "field": "a.c", "op": "absent"}, docs)["passed"]
    assert _one({"part": "p", "key": "d", "field": "none", "op": "absent"}, docs)["passed"]
    assert _one({"part": "p", "key": "gone", "field": "a", "op": "absent"}, docs)["passed"]
    assert _one({"part": "p", "key": "d", "field": "a.b", "op": "absent"}, docs) == check(
        "c", False, "p d.a.b is 1, expected absent")
    assert _one({"part": "p", "key": "d", "field": "a.c", "op": "exists"}, docs)["detail"] == (
        "p d.a.c is missing, expected exists")


def test_numbers_compare_within_half_a_cent():
    docs = {"p": {"d": {"x": 150.004}}}
    assert _one({"part": "p", "key": "d", "field": "x", "op": "=", "value": 150}, docs)["passed"]
    assert _one({"part": "p", "key": "d", "field": "x", "op": "<=", "value": 150}, docs)["passed"]
    assert not _one({"part": "p", "key": "d", "field": "x", "op": ">", "value": 150}, docs)["passed"]
    assert not _one({"part": "p", "key": "d", "field": "x", "op": "<", "value": "a"}, docs)["passed"]


def test_where_takes_ops_and_null_and_count_takes_a_number():
    docs = {"erp": {"po/1": {"total": 900, "approved_by": None}, "po/2": {"total": 90}, "po/3": {"total": 400,
                                                                                           "approved_by": "ana"}}}
    body = {"part": "erp", "find": "po/*", "where": {"total": {">": 150}, "approved_by": None}, "count": 0}
    assert _one(body, docs) == check("c", False, "1 of erp po/* match, expected 0")
    r = evaluate([{"name": "n", "measure": {"label": "POs"}, "check": {"part": "erp", "find": "po/*", "count": ">= 1"}}],
                 docs)[0]
    assert r["passed"] and r["measure"] == {"label": "POs", "value": 3}


def test_a_ref_to_a_missing_doc_and_a_bad_check_fail_with_a_reason():
    ref = {"part": "prod", "key": "version/api", "field": "sha", "op": "=", "ref": {"part": "code", "key": "x", "field": "sha"}}
    assert _one(ref)["detail"] == "code has no x"
    assert _one({"part": "prod", "key": "slo/prod", "field": "error_rate", "op": "~", "value": 1})["detail"] == (
        "can't evaluate: unknown op ~")
    assert _one({"part": "prod"})["detail"] == "can't evaluate: a check needs key, find, all or any"
    assert _one({"part": "tracker", "find": "issue/*", "count": "about 2"})["detail"] == (
        "can't evaluate: count about 2 is not '<op> <n>'")


def test_a_key_check_without_a_field_reads_the_whole_doc():
    assert _one({"part": "code", "key": "branch/main", "op": "exists"})["passed"]


def _cant(body, docs=DOCS) -> str:
    r = _one(body, docs)
    assert r["passed"] is False, r
    return r["detail"]


def test_a_part_not_in_the_world_cant_be_evaluated_whatever_the_op():
    assert _cant({"part": "nope", "key": "x", "op": "absent"}) == "can't evaluate: no part nope in this world"
    assert _cant({"part": "nope", "key": "x", "op": "!=", "value": 1}) == "can't evaluate: no part nope in this world"
    assert _cant({"part": "nope", "find": "*", "count": 0}) == "can't evaluate: no part nope in this world"
    ref = {"part": "code", "key": "branch/main", "field": "sha", "op": "!=", "ref": {"part": "nope", "key": "k"}}
    assert _cant(ref) == "can't evaluate: no part nope in this world"


def test_an_empty_all_or_any_cant_be_evaluated():
    assert _cant({"all": []}) == "can't evaluate: all needs a list of checks"
    assert _cant({"any": []}) == "can't evaluate: any needs a list of checks"


def test_a_ref_to_a_missing_field_cant_be_evaluated():
    body = {"part": "prod", "key": "version/api", "field": "sha", "op": "!=",
            "ref": {"part": "code", "key": "branch/main", "field": "nope"}}
    assert _cant(body) == "can't evaluate: the ref code branch/main.nope is missing"


def test_an_unknown_where_op_cant_be_evaluated_and_an_object_is_compared_with_equals():
    docs = {"p": {"a": {"meta": {"k": 1}}, "b": {"meta": {"k": 2}}}}
    assert _cant({"part": "p", "find": "*", "where": {"meta": {"~": 1}}, "count": 0}, docs) == (
        "can't evaluate: unknown op ~")
    assert _cant({"part": "nope", "find": "*", "where": {"meta": {"~": 1}}, "count": 0}, docs).startswith(
        "can't evaluate:")
    assert _cant({"part": "p", "find": "zz*", "where": {"meta": {"~": 1}}, "count": 0}, docs) == (
        "can't evaluate: unknown op ~")
    assert _cant({"part": "p", "find": "*", "where": {"meta": {">": 0, "<": 9}}}, docs) == (
        "can't evaluate: where meta needs one op, as {op: value}")
    assert _one({"part": "p", "find": "*", "where": {"meta": {"=": {"k": 1}}}, "count": 1}, docs)["passed"]


def test_an_unknown_op_cant_be_evaluated_even_when_the_doc_is_missing():
    assert _cant({"part": "code", "key": "gone", "op": "~", "value": 1}) == "can't evaluate: unknown op ~"


def test_a_compare_without_a_value_or_ref_cant_be_evaluated():
    assert _cant({"part": "code", "key": "branch/main", "field": "sha", "op": "!=", "vaule": "x"}) == (
        "can't evaluate: a key check has no field vaule")
    assert _cant({"part": "code", "key": "branch/main", "field": "sha", "op": "!="}) == (
        "can't evaluate: != needs a value or a ref")


@pytest.mark.parametrize("body", [
    {"key": "branch/main"},                                    # no part
    {"part": "code", "key": ["branch/main"]},                  # unhashable key
    {"part": "code", "find": 5},                               # glob not text
    {"all": 5},                                                # all not a list
    {"any": ["x"]},                                            # an item not a check
    {"part": "code", "find": "*", "where": ["status"]},        # where not an object
    {"part": "code", "key": "branch/main", "ref": "code"},      # ref not an object
    {"part": "code", "key": "branch/main", "op": "=", "ref": {"part": "code"}},  # ref without key
    "not a check",
])
def test_a_malformed_check_fails_alone_with_a_reason(body):
    r = evaluate([{"name": "bad", "check": body}, {"name": "good", "check": {"part": "code", "key": "branch/main"}}],
                 DOCS)
    assert r[0]["passed"] is False and r[0]["detail"].startswith("can't evaluate: "), r[0]
    assert r[1]["passed"] is True


def test_a_malformed_item_or_measure_fails_alone():
    good = {"name": "good", "check": {"part": "code", "key": "branch/main"}}
    r = evaluate(["not an item", {"name": "m", "measure": 5, "check": {"part": "prod", "key": "slo/prod",
                                                                       "field": "error_rate", "op": "<=", "value": 1}},
                  good], DOCS)
    assert [c["passed"] for c in r] == [False, False, True]
    assert r[0]["detail"] == "can't evaluate: a check is an object with a name and a check"
    assert r[1]["detail"].startswith("can't evaluate: ")
