import time

from fastapi.testclient import TestClient

from inventory.app import create_app

# How long the customer takes to pay in the hold test below.
PAYMENT_TIME_S = 0.25


def reserve(client, *items):
    return client.post("/reservations", json={"items": [{"sku": s, "qty": q} for s, q in items]})


def test_a_reservation_holds_stock_and_prices_its_lines(client):
    r = reserve(client, ("LAMP-01", 2), ("MUG-02", 1))
    assert r.status_code == 201
    body = r.json()
    assert body["status"] == "held" and body["total"] == 90.5
    assert body["lines"] == [{"sku": "LAMP-01", "qty": 2, "unit_price": 39.0, "amount": 78.0},
                             {"sku": "MUG-02", "qty": 1, "unit_price": 12.5, "amount": 12.5}]
    assert client.get(f"/reservations/{body['id']}").json() == body


def test_too_few_in_stock_is_a_conflict(client):
    r = reserve(client, ("MUG-02", 4))
    assert (r.status_code, r.json()) == (409, {"detail": {"short": ["MUG-02"]}})


def test_the_same_product_twice_is_one_line(client):
    assert [(l["sku"], l["qty"]) for l in reserve(client, ("MUG-02", 1), ("MUG-02", 1)).json()["lines"]] == [("MUG-02", 2)]


def test_an_unknown_product_cannot_be_reserved(client):
    assert reserve(client, ("NOPE", 1)).status_code == 404


def test_a_bad_quantity_is_refused(client):
    assert reserve(client, ("MUG-02", 0)).status_code == 422


def test_committing_takes_the_stock(client, store):
    rid = reserve(client, ("MUG-02", 2)).json()["id"]
    assert client.post(f"/reservations/{rid}/commit").json()["status"] == "committed"
    assert store.product("MUG-02").on_hand == 1
    assert client.post(f"/reservations/{rid}/commit").json()["status"] == "committed"  # once only
    assert store.product("MUG-02").on_hand == 1


def test_an_expired_hold_frees_its_stock_and_cannot_be_committed(client, clock):
    rid = reserve(client, ("MUG-02", 3)).json()["id"]
    clock.at += 901
    assert client.get(f"/reservations/{rid}").json()["status"] == "expired"
    assert client.get("/stock/MUG-02").json()["available"] == 3
    r = client.post(f"/reservations/{rid}/commit")
    assert (r.status_code, r.json()) == (409, {"detail": f"reservation {rid} has expired"})


def test_an_unknown_reservation_is_not_found(client):
    assert client.get("/reservations/r-none").status_code == 404
    assert client.post("/reservations/r-none/commit").status_code == 404


def test_a_hold_lasts_while_the_customer_pays(store):
    client = TestClient(create_app(store, hold_seconds=1))
    rid = reserve(client, ("LAMP-01", 1)).json()["id"]
    time.sleep(PAYMENT_TIME_S)
    assert client.get(f"/reservations/{rid}").json()["status"] == "held"
