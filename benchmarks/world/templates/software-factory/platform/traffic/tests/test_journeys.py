import random

from conftest import Route

from traffic.client import request
from traffic.journeys import run

PRODUCTS = {"products": [{"sku": "A-1", "available": 0}, {"sku": "B-2", "available": 4},
                         {"sku": "C-3", "available": 9}]}
ORDER = {
    "draw": {"qty": {"int": [1, 3]}, "customer": {"int": [1, 50], "format": "customer-{:03d}"}},
    "steps": [
        {"method": "GET", "service": "shop", "path": "/products",
         "pick": {"sku": {"from": "/products", "where": "available", "take": "/sku"}}},
        {"method": "POST", "service": "shop", "path": "/cart", "save": {"cart_id": "/id"}},
        {"method": "POST", "service": "shop", "path": "/cart/{cart_id}/items", "json": {"sku": "{sku}", "qty": "{qty}"}},
        {"method": "POST", "service": "shop", "path": "/checkout",
         "json": {"cart_id": "{cart_id}", "customer": "{customer}"}, "save": {"order_id": "/order_id"}},
        {"method": "GET", "service": "shop", "path": "/orders/{order_id}"},
    ]}
SHOP = {"/products": PRODUCTS, "POST /cart": Route({"id": "c-9"}, 201), "POST /cart/c-9/items": {"id": "c-9"},
        "POST /checkout": Route({"order_id": 77}, 201), "/orders/77": {"id": 77}}


def sender(sent: list):
    def send(step: dict, method: str, url: str, body):
        sent.append((step["path"], method, url, body))
        return request(method, url, body, timeout_s=2)
    return send


def test_an_order_journey_carries_what_each_answer_gives_into_the_next_request(serve):
    base, seen = serve(SHOP)
    sent: list = []
    assert run(ORDER, {"shop": base}, random.Random("s"), sender(sent)) == "completed"
    assert [f"{r['method']} {r['path']}" for r in seen] == [
        "GET /products", "POST /cart", "POST /cart/c-9/items", "POST /checkout", "GET /orders/77"]
    item, checkout = seen[2]["body"], seen[3]["body"]
    assert item["sku"] in {"B-2", "C-3"} and isinstance(item["qty"], int) and 1 <= item["qty"] <= 3
    assert checkout["cart_id"] == "c-9" and checkout["customer"].startswith("customer-")
    assert sent[2][0] == "/cart/{cart_id}/items"         # the step's template names it in telemetry


def test_the_same_random_stream_makes_the_same_choices(serve):
    base, seen = serve(SHOP)
    for _ in range(2):
        run(ORDER, {"shop": base}, random.Random("same"), sender([]))
    assert [seen[i]["body"] for i in (2, 3)] == [seen[i]["body"] for i in (7, 8)]


def test_nothing_to_pick_abandons_the_journey(serve):
    base, seen = serve({**SHOP, "/products": {"products": [{"sku": "A-1", "available": 0}]}})
    assert run(ORDER, {"shop": base}, random.Random(1), sender([])) == "abandoned" and len(seen) == 1


def test_an_answer_that_is_not_a_success_ends_the_journey(serve):
    base, seen = serve({**SHOP, "POST /checkout": Route({"detail": "down"}, 502)})
    assert run(ORDER, {"shop": base}, random.Random(1), sender([])) == "failed" and len(seen) == 4
