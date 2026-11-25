"""The services storefront calls, and what it reads from their answers (`contracts/` lists it for them)."""
import httpx


class Rejected(Exception):
    """A provider refused the request; `status` and `detail` are its answer."""

    def __init__(self, status: int, detail):
        super().__init__(f"{status}: {detail}")
        self.status, self.detail = status, detail


def answer(r: httpx.Response, refusals: tuple[int, ...] = ()) -> dict | None:
    if r.status_code == 404:
        return None
    if r.status_code in refusals:
        raise Rejected(r.status_code, r.json()["detail"])
    r.raise_for_status()
    return r.json()


class InventoryClient:
    name = "inventory"

    def __init__(self, http: httpx.Client):
        self.http = http

    def stock(self) -> list[dict]:
        return answer(self.http.get("/stock"))["items"]

    def item(self, sku: str) -> dict | None:
        return answer(self.http.get(f"/stock/{sku}"))

    def reserve(self, items: list[dict]) -> dict:
        return answer(self.http.post("/reservations", json={"items": items}), refusals=(409,))


class OrdersClient:
    name = "orders"

    def __init__(self, http: httpx.Client):
        self.http = http

    def place(self, reservation_id: str, customer: str) -> dict:
        return answer(self.http.post("/orders", json={"reservation_id": reservation_id, "customer": customer}),
                      refusals=(409, 422))

    def get(self, order_id: int) -> dict | None:
        return answer(self.http.get(f"/orders/{order_id}"))
