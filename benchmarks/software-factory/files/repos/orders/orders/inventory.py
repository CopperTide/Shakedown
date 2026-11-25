"""The inventory service, as orders uses it: read a reservation, commit it."""
import httpx


class ReservationConflict(Exception):
    pass


class Inventory:
    def __init__(self, http: httpx.Client):
        self.http = http

    def reservation(self, reservation_id: str) -> dict | None:
        r = self.http.get(f"/reservations/{reservation_id}")
        if r.status_code == 404:
            return None
        r.raise_for_status()
        return r.json()

    def commit(self, reservation_id: str) -> dict:
        r = self.http.post(f"/reservations/{reservation_id}/commit")
        if r.status_code == 409:
            raise ReservationConflict(r.json()["detail"])
        r.raise_for_status()
        return r.json()
