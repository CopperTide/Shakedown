PLACED = {"id": 1, "customer": "c-42", "reservation_id": "r-1", "status": "placed", "total": 90.5, "notes": "",
          "created_at": "2026-03-10T06:00:00Z",
          "lines": [{"sku": "LAMP-01", "qty": 2, "unit_price": 39.0, "amount": 78.0},
                    {"sku": "MUG-02", "qty": 1, "unit_price": 12.5, "amount": 12.5}]}


def place(client, reservation_id="r-1", customer="c-42", **extra):
    return client.post("/orders", json={"reservation_id": reservation_id, "customer": customer, **extra})


def test_an_order_is_placed_from_a_held_reservation_and_commits_it(client, inventory):
    r = place(client)
    assert (r.status_code, r.json()) == (201, PLACED)
    assert inventory.commits == ["r-1"]
    assert client.get("/orders/1").json() == PLACED


def test_placing_the_same_reservation_again_gives_the_same_order(client, inventory):
    place(client)
    r = place(client)
    assert (r.status_code, r.json()["id"]) == (200, 1)
    assert inventory.commits == ["r-1"]


def test_notes_are_kept(client):
    assert place(client, notes="leave at the door").json()["notes"] == "leave at the door"


def test_an_expired_reservation_cannot_be_ordered(client):
    r = place(client, reservation_id="r-old")
    assert (r.status_code, r.json()) == (409, {"detail": "reservation r-old is expired"})


def test_an_unknown_reservation_cannot_be_ordered(client):
    r = place(client, reservation_id="r-none")
    assert (r.status_code, r.json()) == (422, {"detail": "no reservation r-none"})


def test_an_unknown_order_is_not_found(client):
    assert client.get("/orders/99").status_code == 404


def test_a_customers_orders_newest_first(client, inventory):
    inventory.reservations["r-2"] = {**inventory.reservations["r-1"], "id": "r-2", "status": "held"}
    place(client)
    place(client, reservation_id="r-2")
    place(client, reservation_id="r-2", customer="someone-else")
    assert [o["id"] for o in client.get("/orders", params={"customer": "c-42"}).json()["orders"]] == [2, 1]
