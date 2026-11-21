"""Orders in Postgres."""
from .models import Line, Order

ORDER = "SELECT id, customer, reservation_id, status, total, notes, created_at FROM orders"


class PgStore:
    def __init__(self, pool, metrics):
        self.pool, self.metrics = pool, metrics

    def create(self, order: Order) -> Order:
        """Insert `order`; an order already placed from its reservation is returned instead."""
        with self.metrics.query("insert_order"), self.pool.connection() as conn, conn.transaction():
            row = conn.execute(
                "INSERT INTO orders (customer, reservation_id, status, total, notes, created_at) "
                "VALUES (%s, %s, %s, %s, %s, %s) ON CONFLICT (reservation_id) DO NOTHING RETURNING id",
                (order.customer, order.reservation_id, order.status, order.total, order.notes,
                 order.created_at)).fetchone()
            if row is not None:
                with conn.cursor() as cur:
                    cur.executemany("INSERT INTO order_lines (order_id, sku, qty, unit_price, amount) "
                                    "VALUES (%s, %s, %s, %s, %s)",
                                    [(row[0], li.sku, li.qty, li.unit_price, li.amount) for li in order.lines])
        return self.get(row[0]) if row else self.by_reservation(order.reservation_id)

    def get(self, order_id: int) -> Order | None:
        found = self._orders(f"{ORDER} WHERE id = %s", (order_id,), "select_order")
        return found[0] if found else None

    def by_reservation(self, reservation_id: str) -> Order | None:
        found = self._orders(f"{ORDER} WHERE reservation_id = %s", (reservation_id,), "select_order")
        return found[0] if found else None

    def by_customer(self, customer: str, limit: int) -> list[Order]:
        return self._orders(f"{ORDER} WHERE customer = %s ORDER BY id DESC LIMIT %s", (customer, limit),
                            "select_customer_orders")

    def _orders(self, sql: str, params: tuple, operation: str) -> list[Order]:
        with self.metrics.query(operation), self.pool.connection() as conn:
            rows = conn.execute(sql, params).fetchall()
            ids = [r[0] for r in rows]
            lines: dict[int, list[Line]] = {i: [] for i in ids}
            for order_id, *line in conn.execute("SELECT order_id, sku, qty, unit_price, amount FROM order_lines "
                                                "WHERE order_id = ANY(%s) ORDER BY order_id, sku", (ids,)):
                lines[order_id].append(Line(*line))
        return [Order(*r, lines=tuple(lines[r[0]])) for r in rows]
