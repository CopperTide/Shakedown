def test_stock_lists_every_product_with_its_price_and_availability(client):
    assert client.get("/stock").json() == {"items": [
        {"sku": "LAMP-01", "name": "Desk lamp", "unit_price": 39.0, "currency": "USD", "available": 12},
        {"sku": "MUG-02", "name": "Enamel mug", "unit_price": 12.5, "currency": "USD", "available": 3},
    ]}


def test_one_product(client):
    assert client.get("/stock/MUG-02").json() == {
        "sku": "MUG-02", "name": "Enamel mug", "unit_price": 12.5, "currency": "USD", "available": 3}


def test_an_unknown_product_is_not_found(client):
    r = client.get("/stock/NOPE")
    assert (r.status_code, r.json()) == (404, {"detail": "no product NOPE"})


def test_held_stock_is_not_available(client):
    client.post("/reservations", json={"items": [{"sku": "MUG-02", "qty": 2}]})
    assert client.get("/stock/MUG-02").json()["available"] == 1
