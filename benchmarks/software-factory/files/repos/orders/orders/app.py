"""The orders API."""
import logging
from datetime import UTC, datetime
from decimal import Decimal

import httpx
from fastapi import FastAPI, HTTPException, Query
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field

from platform_lib import WorldClock

from .inventory import Inventory, ReservationConflict
from .models import Line, Order

log = logging.getLogger(__name__)


class OrderRequest(BaseModel):
    reservation_id: str = Field(min_length=1)
    customer: str = Field(min_length=1)
    notes: str = ""


def order_body(order: Order) -> dict:
    lines = [{"sku": line.sku, "qty": line.qty, "unit_price": float(line.unit_price), "amount": float(line.amount)}
             for line in order.lines]
    return {"id": order.id, "customer": order.customer, "reservation_id": order.reservation_id,
            "status": order.status, "total": float(order.total), "notes": order.notes,
            "created_at": order.created_at.isoformat().replace("+00:00", "Z"), "lines": lines}


def from_reservation(request: OrderRequest, reservation: dict, now: datetime) -> Order:
    lines = tuple(Line(line["sku"], line["qty"], Decimal(str(line["unit_price"])), Decimal(str(line["amount"])))
                  for line in reservation["lines"])
    return Order(None, request.customer, request.reservation_id, "placed", Decimal(str(reservation["total"])),
                 request.notes, now, lines)


def create_app(store, inventory: Inventory, clock=None) -> FastAPI:
    clock = clock or WorldClock()
    app = FastAPI(title="orders")

    @app.exception_handler(httpx.HTTPError)
    def inventory_down(request, exc):
        log.exception("inventory request failed")
        return JSONResponse({"detail": "inventory is unavailable"}, status_code=502)

    @app.post("/orders", status_code=201)
    def place_order(request: OrderRequest):
        existing = store.by_reservation(request.reservation_id)
        if existing:
            return JSONResponse(order_body(existing), status_code=200)
        reservation = inventory.reservation(request.reservation_id)
        if reservation is None:
            raise HTTPException(422, f"no reservation {request.reservation_id}")
        if reservation["status"] != "held":
            raise HTTPException(409, f"reservation {request.reservation_id} is {reservation['status']}")
        try:
            inventory.commit(request.reservation_id)
        except ReservationConflict as e:
            raise HTTPException(409, str(e)) from None
        now = datetime.fromtimestamp(clock.now(), UTC).replace(microsecond=0)
        return order_body(store.create(from_reservation(request, reservation, now)))

    @app.get("/orders/{order_id}")
    def get_order(order_id: int):
        order = store.get(order_id)
        if order is None:
            raise HTTPException(404, f"no order {order_id}")
        return order_body(order)

    @app.get("/orders")
    def list_orders(customer: str, limit: int = Query(20, ge=1, le=100)):
        return {"orders": [order_body(o) for o in store.by_customer(customer, limit)]}

    return app
