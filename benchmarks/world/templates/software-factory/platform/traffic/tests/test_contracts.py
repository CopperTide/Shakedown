import json

from conftest import Route

from traffic.contracts import check, load, run_all

ORDERS = {"consumer": "storefront", "provider": "orders",
          "calls": [{"method": "GET", "path": "/orders/1", "fields": {"total": "number"}}]}


def test_a_contract_reports_a_missing_field(serve_json):
    assert check({"provider": "orders", "calls": [{"method": "GET", "path": "/orders/1", "fields": {"total": "number"}}]},
                 serve_json({"/orders/1": {"total_cents": 1200}})) == ["GET /orders/1: field total missing"]


def test_a_contract_reports_a_wrong_type_and_passes_extra_fields(serve_json):
    base = serve_json({"/orders/1": {"total": "12.00", "extra": 1}})
    assert check(ORDERS, base) == ["GET /orders/1: total is string, expected number"]
    assert check(ORDERS, serve_json({"/orders/1": {"total": 12, "extra": 1}})) == []


def test_an_error_status_a_body_that_is_not_json_and_no_answer_are_failures(serve_json, closed_port):
    assert check(ORDERS, serve_json({"/orders/1": Route({"detail": "no"}, 404)})) == \
        ["GET /orders/1: status 404, expected 2xx"]
    assert check(ORDERS, serve_json({"/orders/1": Route(b"<html>")})) == ["GET /orders/1: the body is not JSON"]
    assert check(ORDERS, closed_port) == ["GET /orders/1: no answer (connection)"]


def test_every_contract_in_the_folder_runs_against_its_provider(tmp_path, serve_json):
    (tmp_path / "storefront").mkdir()
    (tmp_path / "storefront" / "orders.json").write_text(json.dumps(ORDERS))
    (tmp_path / "storefront" / "billing.json").write_text(json.dumps({**ORDERS, "provider": "billing"}))
    contracts = load(tmp_path)
    assert [(c["consumer"], c["provider"]) for c in contracts] == [("storefront", "billing"), ("storefront", "orders")]
    doc = run_all(contracts, {"orders": serve_json({"/orders/1": {"total": 1}})}, at_world_ms=5)
    assert doc == {"storefront-billing": {"passed": False, "failures": ["no address for the provider billing"],
                                          "at_world_ms": 5},
                   "storefront-orders": {"passed": True, "failures": [], "at_world_ms": 5}}
