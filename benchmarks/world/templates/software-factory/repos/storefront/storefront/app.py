"""The storefront API."""
import logging
import uuid

import httpx
from fastapi import FastAPI, HTTPException
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field

from .upstream import InventoryClient, OrdersClient, Rejected

log = logging.getLogger(__name__)


class CartItem(BaseModel):
    sku: str
    qty: int = Field(gt=0, le=100)


class Checkout(BaseModel):
    cart_id: str
    customer: str = Field(min_length=1)


def cart_body(cart_id: str, items: dict[str, int]) -> dict:
    return {"id": cart_id, "items": [{"sku": sku, "qty": qty} for sku, qty in sorted(items.items())]}


def add_error_handlers(app: FastAPI) -> None:
    @app.exception_handler(httpx.HTTPError)
    def upstream_down(request, exc: httpx.HTTPError):
        service = exc.request.url.host.split(".")[0] if exc.request else "a service"
        log.exception("request to %s failed", service)
        return JSONResponse({"detail": f"{service} is unavailable"}, status_code=502)

    @app.exception_handler(Rejected)
    def rejected(request, exc: Rejected):
        return JSONResponse({"detail": exc.detail}, status_code=exc.status)


def create_app(carts, inventory: InventoryClient, orders: OrdersClient) -> FastAPI:
    app = FastAPI(title="storefront")
    add_error_handlers(app)

    def cart(cart_id: str) -> dict[str, int]:
        items = carts.items(cart_id)
        if items is None:
            raise HTTPException(404, f"no cart {cart_id}")
        return items

    @app.get("/products")
    def products():
        return {"products": [{"sku": s["sku"], "name": s["name"], "price": s["unit_price"],
                              "currency": s["currency"], "available": s["available"]} for s in inventory.stock()]}

    @app.post("/cart", status_code=201)
    def new_cart():
        cart_id = f"c-{uuid.uuid4().hex[:12]}"
        carts.create(cart_id)
        return cart_body(cart_id, {})

    @app.get("/cart/{cart_id}")
    def get_cart(cart_id: str):
        return cart_body(cart_id, cart(cart_id))

    @app.post("/cart/{cart_id}/items")
    def add_item(cart_id: str, item: CartItem):
        cart(cart_id)
        if inventory.item(item.sku) is None:
            raise HTTPException(404, f"no product {item.sku}")
        carts.add(cart_id, item.sku, item.qty)
        return cart_body(cart_id, cart(cart_id))

    @app.post("/checkout", status_code=201)
    def checkout(request: Checkout):
        items = cart(request.cart_id)
        if not items:
            raise HTTPException(400, "the cart is empty")
        reservation = inventory.reserve(cart_body(request.cart_id, items)["items"])
        order = orders.place(reservation["id"], request.customer)
        carts.delete(request.cart_id)
        return {"order_id": order["id"], "status": order["status"], "total": order["total"]}

    @app.get("/orders/{order_id}")
    def get_order(order_id: int):
        order = orders.get(order_id)
        if order is None:
            raise HTTPException(404, f"no order {order_id}")
        return {"id": order["id"], "status": order["status"], "total": order["total"],
                "placed_at": order["created_at"]}

    return app
