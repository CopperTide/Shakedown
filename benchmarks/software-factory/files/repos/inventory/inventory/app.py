"""The inventory API."""
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

from platform_lib import WorldClock

from .models import Product, Reservation
from .pricing import unit_price
from .reservations import Expired, OutOfStock, Reservations, UnknownProduct


class Wanted(BaseModel):
    sku: str
    qty: int = Field(gt=0)


class ReservationRequest(BaseModel):
    items: list[Wanted] = Field(min_length=1)


def stock_item(product: Product, held: dict[str, int]) -> dict:
    return {"sku": product.sku, "name": product.name, "unit_price": float(unit_price(product)),
            "currency": product.currency, "available": product.on_hand - held.get(product.sku, 0)}


def reservation_body(reservation: Reservation, status: str) -> dict:
    lines = [{"sku": line.sku, "qty": line.qty, "unit_price": float(line.unit_price), "amount": float(line.amount)}
             for line in reservation.lines]
    return {"id": reservation.id, "status": status, "lines": lines, "total": float(reservation.total),
            "expires_at": reservation.expires_at}


def create_app(store, clock=None, hold_seconds: int = 900) -> FastAPI:
    clock = clock or WorldClock()
    holds = Reservations(store, clock, hold_seconds)
    app = FastAPI(title="inventory")

    def found(reservation_id: str) -> Reservation:
        reservation = store.reservation(reservation_id)
        if reservation is None:
            raise HTTPException(404, f"no reservation {reservation_id}")
        return reservation

    @app.get("/stock")
    def list_stock():
        held = store.held(clock.now())
        return {"items": [stock_item(p, held) for p in store.products()]}

    @app.get("/stock/{sku}")
    def get_stock(sku: str):
        product = store.product(sku)
        if product is None:
            raise HTTPException(404, f"no product {sku}")
        return stock_item(product, store.held(clock.now()))

    @app.post("/reservations", status_code=201)
    def reserve(request: ReservationRequest):
        try:
            reservation = holds.reserve([(w.sku, w.qty) for w in request.items])
        except UnknownProduct as e:
            raise HTTPException(404, str(e)) from None
        except OutOfStock as e:
            raise HTTPException(409, {"short": e.short}) from None
        return reservation_body(reservation, "held")

    @app.get("/reservations/{reservation_id}")
    def get_reservation(reservation_id: str):
        reservation = found(reservation_id)
        return reservation_body(reservation, holds.status(reservation))

    @app.post("/reservations/{reservation_id}/commit")
    def commit(reservation_id: str):
        try:
            committed = holds.commit(found(reservation_id))
        except Expired as e:
            raise HTTPException(409, str(e)) from None
        return reservation_body(committed, "committed")

    return app
