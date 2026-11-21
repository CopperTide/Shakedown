"""An in-memory store with the same behaviour as `PgStore`, and an inventory service that answers like the real
one for the reservations it is given."""
from dataclasses import replace

import httpx

from orders.models import Order

RESERVATION = {"id": "r-1", "status": "held", "total": 90.5, "expires_at": 1_773_123_300,
               "lines": [{"sku": "LAMP-01", "qty": 2, "unit_price": 39.0, "amount": 78.0},
                         {"sku": "MUG-02", "qty": 1, "unit_price": 12.5, "amount": 12.5}]}


class MemoryStore:
    def __init__(self):
        self.rows: dict[int, Order] = {}

    def create(self, order: Order) -> Order:
        made = replace(order, id=len(self.rows) + 1)
        self.rows[made.id] = made
        return made

    def get(self, order_id: int) -> Order | None:
        return self.rows.get(order_id)

    def by_reservation(self, reservation_id: str) -> Order | None:
        return next((o for o in self.rows.values() if o.reservation_id == reservation_id), None)

    def by_customer(self, customer: str, limit: int) -> list[Order]:
        mine = [o for o in self.rows.values() if o.customer == customer]
        return sorted(mine, key=lambda o: o.id, reverse=True)[:limit]


class FakeInventory:
    """Reservations by id; a commit marks one committed, as inventory does."""

    def __init__(self, *reservations: dict):
        self.reservations = {r["id"]: dict(r) for r in reservations}
        self.commits: list[str] = []

    def handle(self, request: httpx.Request) -> httpx.Response:
        parts = request.url.path.strip("/").split("/")
        if parts[0] != "reservations" or parts[1] not in self.reservations:
            return httpx.Response(404, json={"detail": "not found"})
        reservation = self.reservations[parts[1]]
        if request.method == "POST" and parts[2:] == ["commit"]:
            if reservation["status"] == "expired":
                return httpx.Response(409, json={"detail": f"reservation {parts[1]} has expired"})
            self.commits.append(parts[1])
            reservation["status"] = "committed"
        return httpx.Response(200, json=reservation)

    def client(self) -> httpx.Client:
        return httpx.Client(base_url="http://inventory.test", transport=httpx.MockTransport(self.handle))
