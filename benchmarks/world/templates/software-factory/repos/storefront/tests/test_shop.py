def new_cart(client, *items) -> str:
    cart_id = client.post("/cart").json()["id"]
    for sku, qty in items:
        client.post(f"/cart/{cart_id}/items", json={"sku": sku, "qty": qty})
    return cart_id


def test_products_show_price_and_availability(client):
    assert client.get("/products").json() == {"products": [
        {"sku": "LAMP-01", "name": "Desk lamp", "price": 39.0, "currency": "USD", "available": 12},
        {"sku": "MUG-02", "name": "Enamel mug", "price": 12.5, "currency": "USD", "available": 3}]}


def test_a_cart_collects_items(client):
    cart_id = new_cart(client, ("MUG-02", 1), ("LAMP-01", 1), ("MUG-02", 1))
    assert client.get(f"/cart/{cart_id}").json() == {"id": cart_id, "items": [
        {"sku": "LAMP-01", "qty": 1}, {"sku": "MUG-02", "qty": 2}]}


def test_an_unknown_product_cannot_go_in_a_cart(client):
    cart_id = new_cart(client)
    assert client.post(f"/cart/{cart_id}/items", json={"sku": "NOPE", "qty": 1}).status_code == 404


def test_an_unknown_cart_is_not_found(client):
    assert client.get("/cart/c-none").status_code == 404


def test_checkout_reserves_orders_and_empties_the_cart(client, inventory):
    cart_id = new_cart(client, ("LAMP-01", 2), ("MUG-02", 1))
    r = client.post("/checkout", json={"cart_id": cart_id, "customer": "c-42"})
    assert (r.status_code, r.json()) == (201, {"order_id": 2, "status": "placed", "total": 90.5})
    assert inventory.reserved == [[{"sku": "LAMP-01", "qty": 2}, {"sku": "MUG-02", "qty": 1}]]
    assert client.get(f"/cart/{cart_id}").status_code == 404


def test_checkout_of_too_many_is_a_conflict_and_keeps_the_cart(client):
    cart_id = new_cart(client, ("MUG-02", 4))
    r = client.post("/checkout", json={"cart_id": cart_id, "customer": "c-42"})
    assert (r.status_code, r.json()) == (409, {"detail": {"short": ["MUG-02"]}})
    assert client.get(f"/cart/{cart_id}").status_code == 200


def test_an_empty_cart_cannot_be_checked_out(client):
    r = client.post("/checkout", json={"cart_id": new_cart(client), "customer": "c-42"})
    assert (r.status_code, r.json()) == (400, {"detail": "the cart is empty"})


def test_an_order_shows_its_status_and_total(client):
    assert client.get("/orders/1").json() == {"id": 1, "status": "shipped", "total": 25.0,
                                              "placed_at": "2026-03-09T18:20:00Z"}
    assert client.get("/orders/99").status_code == 404


def test_a_service_that_is_down_is_a_bad_gateway(client, inventory):
    inventory.down = True
    r = client.get("/products")
    assert (r.status_code, r.json()) == (502, {"detail": "inventory is unavailable"})
