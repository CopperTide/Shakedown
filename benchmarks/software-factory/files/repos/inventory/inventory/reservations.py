"""Holds on stock: placed at checkout, held for `hold_seconds`, then committed by the order or left to expire."""
import uuid

from .models import Line, Reservation
from .pricing import unit_price


class UnknownProduct(Exception):
    pass


class OutOfStock(Exception):
    def __init__(self, short: list[str]):
        super().__init__(f"too few in stock: {', '.join(short)}")
        self.short = short


class Expired(Exception):
    pass


class Reservations:
    def __init__(self, store, clock, hold_seconds: int):
        self.store, self.clock, self.hold_seconds = store, clock, hold_seconds

    def reserve(self, wanted: list[tuple[str, int]]) -> Reservation:
        totals: dict[str, int] = {}
        for sku, qty in wanted:
            totals[sku] = totals.get(sku, 0) + qty
        lines = tuple(Line(sku, qty, unit_price(self._product(sku))) for sku, qty in totals.items())
        now = self.clock.now()
        reservation = Reservation(f"r-{uuid.uuid4().hex[:12]}", lines, int(now) + self.hold_seconds, "held")
        short = self.store.hold(reservation, now)
        if short:
            raise OutOfStock(short)
        return reservation

    def status(self, reservation: Reservation) -> str:
        if reservation.status == "held" and self.clock.now() >= reservation.expires_at:
            return "expired"
        return reservation.status

    def commit(self, reservation: Reservation) -> Reservation:
        state = self.status(reservation)
        if state == "expired":
            raise Expired(f"reservation {reservation.id} has expired")
        if state == "held":
            self.store.commit(reservation.id)
        return self.store.reservation(reservation.id)

    def _product(self, sku: str):
        product = self.store.product(sku)
        if product is None:
            raise UnknownProduct(f"no product {sku}")
        return product
