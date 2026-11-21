"""Stock and holds in Postgres."""
from .models import Line, Product, Reservation

PRODUCT = "SELECT sku, name, list_price, discount_pct, currency, on_hand FROM products"
HELD = ("SELECT l.sku, sum(l.qty) FROM reservation_lines l JOIN reservations r ON r.id = l.reservation_id "
        "WHERE r.status = 'held' AND r.expires_at > %s GROUP BY l.sku")


class PgStore:
    def __init__(self, pool, metrics):
        self.pool, self.metrics = pool, metrics

    def products(self) -> list[Product]:
        with self.metrics.query("select_products"), self.pool.connection() as conn:
            return [Product(*row) for row in conn.execute(f"{PRODUCT} ORDER BY sku")]

    def product(self, sku: str) -> Product | None:
        with self.metrics.query("select_product"), self.pool.connection() as conn:
            row = conn.execute(f"{PRODUCT} WHERE sku = %s", (sku,)).fetchone()
        return Product(*row) if row else None

    def held(self, now_s: float) -> dict[str, int]:
        with self.metrics.query("select_held"), self.pool.connection() as conn:
            return {sku: int(qty) for sku, qty in conn.execute(HELD, (now_s,))}

    def hold(self, reservation: Reservation, now_s: float) -> list[str]:
        """Place `reservation` unless a line wants more than is on hand and not held; the short skus."""
        skus = [line.sku for line in reservation.lines]
        with self.metrics.query("insert_reservation"), self.pool.connection() as conn, conn.transaction():
            on_hand = dict(conn.execute("SELECT sku, on_hand FROM products WHERE sku = ANY(%s) ORDER BY sku "
                                        "FOR UPDATE", (skus,)).fetchall())
            held = {sku: int(qty) for sku, qty in conn.execute(HELD, (now_s,))}
            short = [line.sku for line in reservation.lines if on_hand[line.sku] - held.get(line.sku, 0) < line.qty]
            if not short:
                self._insert(conn, reservation)
        return short

    def reservation(self, reservation_id: str) -> Reservation | None:
        with self.metrics.query("select_reservation"), self.pool.connection() as conn:
            row = conn.execute("SELECT id, expires_at, status FROM reservations WHERE id = %s",
                               (reservation_id,)).fetchone()
            if row is None:
                return None
            lines = conn.execute("SELECT sku, qty, unit_price FROM reservation_lines WHERE reservation_id = %s "
                                 "ORDER BY sku", (reservation_id,)).fetchall()
        return Reservation(row[0], tuple(Line(*line) for line in lines), row[1], row[2])

    def commit(self, reservation_id: str) -> None:
        with self.metrics.query("commit_reservation"), self.pool.connection() as conn, conn.transaction():
            done = conn.execute("UPDATE reservations SET status = 'committed' WHERE id = %s AND status = 'held' "
                                "RETURNING id", (reservation_id,)).fetchone()
            if done:
                conn.execute("UPDATE products p SET on_hand = p.on_hand - l.qty FROM reservation_lines l "
                             "WHERE l.reservation_id = %s AND p.sku = l.sku", (reservation_id,))

    @staticmethod
    def _insert(conn, reservation: Reservation) -> None:
        conn.execute("INSERT INTO reservations (id, status, expires_at) VALUES (%s, %s, %s)",
                     (reservation.id, reservation.status, reservation.expires_at))
        with conn.cursor() as cur:
            cur.executemany("INSERT INTO reservation_lines (reservation_id, sku, qty, unit_price) "
                            "VALUES (%s, %s, %s, %s)",
                            [(reservation.id, line.sku, line.qty, line.unit_price) for line in reservation.lines])
