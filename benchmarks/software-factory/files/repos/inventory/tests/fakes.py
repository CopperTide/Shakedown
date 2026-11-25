"""An in-memory store with the same behaviour as `PgStore`, a catalog to fill it with, and a clock tests move."""
from dataclasses import replace
from decimal import Decimal

from inventory.models import Product, Reservation

CATALOG = [
    Product(sku="LAMP-01", name="Desk lamp", list_price=Decimal("39.00"), discount_pct=0, currency="USD", on_hand=12),
    Product(sku="MUG-02", name="Enamel mug", list_price=Decimal("12.50"), discount_pct=0, currency="USD", on_hand=3),
]


class SteppedClock:
    def __init__(self, at: float = 1_773_122_400.0):
        self.at = at

    def now(self) -> float:
        return self.at


class MemoryStore:
    def __init__(self, products=CATALOG):
        self.items = {p.sku: p for p in products}
        self.holds: dict[str, Reservation] = {}

    def products(self) -> list[Product]:
        return sorted(self.items.values(), key=lambda p: p.sku)

    def product(self, sku: str) -> Product | None:
        return self.items.get(sku)

    def held(self, now_s: float) -> dict[str, int]:
        out: dict[str, int] = {}
        for r in self.holds.values():
            if r.status == "held" and now_s < r.expires_at:
                for line in r.lines:
                    out[line.sku] = out.get(line.sku, 0) + line.qty
        return out

    def hold(self, reservation: Reservation, now_s: float) -> list[str]:
        held = self.held(now_s)
        short = [line.sku for line in reservation.lines
                 if self.items[line.sku].on_hand - held.get(line.sku, 0) < line.qty]
        if not short:
            self.holds[reservation.id] = reservation
        return short

    def reservation(self, reservation_id: str) -> Reservation | None:
        return self.holds.get(reservation_id)

    def commit(self, reservation_id: str) -> None:
        r = self.holds[reservation_id]
        for line in r.lines:
            p = self.items[line.sku]
            self.items[line.sku] = replace(p, on_hand=p.on_hand - line.qty)
        self.holds[reservation_id] = replace(r, status="committed")
