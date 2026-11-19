from platform_lib.contracts import failures

CALL = {"method": "GET", "path": "/orders/1", "fields": {"total": "number", "status": "string", "lines": "array"}}


def test_a_body_with_every_field_of_its_type_passes():
    assert failures(CALL, {"total": 12, "status": "placed", "lines": [], "extra": True}) == []


def test_a_missing_field_and_a_wrong_type_are_each_reported():
    assert failures(CALL, {"total_cents": 1200, "status": 3, "lines": []}) == [
        "GET /orders/1: field total missing", "GET /orders/1: status is number, expected string"]


def test_a_boolean_is_not_a_number_and_a_body_must_be_an_object():
    assert failures(CALL, {"total": True, "status": "x", "lines": []}) == [
        "GET /orders/1: total is boolean, expected number"]
    assert failures(CALL, [1]) == ["GET /orders/1: the body is array, expected object"]
