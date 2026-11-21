"""In-memory carts with the same behaviour as `PgStore`, and inventory and orders services that answer as the real
ones do (`test_contracts.py` holds them to what `contracts/` says storefront needs)."""
import json

import httpx

STOCK = [{"sku": "LAMP-01", "name": "Desk lamp", "unit_price": 39.0, "currency": "USD", "available": 12},
         {"sku": "MUG-02", "name": "Enamel mug", "unit_price": 12.5, "currency": "USD", "available": 3}]


class MemoryCarts:
    def __init__(self):
        self.carts: dict[str, dict[str, int]] = {}

    def create(self, cart_id: str) -> None:
        self.carts[cart_id] = {}

    def items(self, cart_id: str) -> dict[str, int] | None:
        return dict(self.carts[cart_id]) if cart_id in self.carts else None

    def add(self, cart_id: str, sku: str, qty: int) -> None:
        self.carts[cart_id][sku] = self.carts[cart_id].get(sku, 0) + qty

    def delete(self, cart_id: str) -> None:
        self.carts.pop(cart_id, None)


def json_response(status: int, body) -> httpx.Response:
    return httpx.Response(status, json=body)


class FakeInventory:
    def __init__(self):
        self.stock = {s["sku"]: dict(s) for s in STOCK}
        self.reserved: list[list[dict]] = []
        self.down = False

    def handle(self, request: httpx.Request) -> httpx.Response:
        if self.down:
            raise httpx.ConnectError("connection refused", request=request)
        path = request.url.path
        if path == "/stock":
            return json_response(200, {"items": list(self.stock.values())})
        if path.startswith("/stock/"):
            item = self.stock.get(path.removeprefix("/stock/"))
            return json_response(200, item) if item else json_response(404, {"detail": "no product"})
        if path == "/reservations" and request.method == "POST":
            return self._reserve(json.loads(request.content)["items"])
        return json_response(404, {"detail": "not found"})

    def _reserve(self, items: list[dict]) -> httpx.Response:
        short = [i["sku"] for i in items if self.stock[i["sku"]]["available"] < i["qty"]]
        if short:
            return json_response(409, {"detail": {"short": short}})
        self.reserved.append(items)
        lines = [{**i, "unit_price": self.stock[i["sku"]]["unit_price"],
                  "amount": self.stock[i["sku"]]["unit_price"] * i["qty"]} for i in items]
        return json_response(201, {"id": f"r-{len(self.reserved)}", "status": "held", "lines": lines,
                                   "total": sum(line["amount"] for line in lines), "expires_at": 1_773_123_300})


class FakeOrders:
    def __init__(self, inventory: FakeInventory):
        self.inventory = inventory
        self.orders: dict[int, dict] = {1: {"id": 1, "customer": "c-1", "reservation_id": "r-0", "status": "shipped",
                                            "total": 25.0, "notes": "", "created_at": "2026-03-09T18:20:00Z",
                                            "lines": []}}

    def handle(self, request: httpx.Request) -> httpx.Response:
        if request.url.path == "/orders" and request.method == "POST":
            body = json.loads(request.content)
            order = {"id": len(self.orders) + 1, "customer": body["customer"], "status": "placed",
                     "reservation_id": body["reservation_id"], "total": self._total(body["reservation_id"]),
                     "notes": "", "created_at": "2026-03-10T06:00:00Z", "lines": []}
            self.orders[order["id"]] = order
            return json_response(201, order)
        order_id = request.url.path.removeprefix("/orders/")
        order = self.orders.get(int(order_id)) if order_id.isdigit() else None
        return json_response(200, order) if order else json_response(404, {"detail": "no order"})

    def _total(self, reservation_id: str) -> float:
        items = self.inventory.reserved[int(reservation_id.removeprefix("r-")) - 1]
        return sum(self.inventory.stock[i["sku"]]["unit_price"] * i["qty"] for i in items)


def client_for(fake, base_url: str) -> httpx.Client:
    return httpx.Client(base_url=base_url, transport=httpx.MockTransport(fake.handle))
