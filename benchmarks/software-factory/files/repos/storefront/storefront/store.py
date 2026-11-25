"""Carts in Postgres."""


class PgCarts:
    def __init__(self, pool, metrics):
        self.pool, self.metrics = pool, metrics

    def create(self, cart_id: str) -> None:
        with self.metrics.query("insert_cart"), self.pool.connection() as conn:
            conn.execute("INSERT INTO carts (id) VALUES (%s)", (cart_id,))

    def items(self, cart_id: str) -> dict[str, int] | None:
        with self.metrics.query("select_cart"), self.pool.connection() as conn:
            if conn.execute("SELECT 1 FROM carts WHERE id = %s", (cart_id,)).fetchone() is None:
                return None
            return dict(conn.execute("SELECT sku, qty FROM cart_items WHERE cart_id = %s", (cart_id,)).fetchall())

    def add(self, cart_id: str, sku: str, qty: int) -> None:
        with self.metrics.query("upsert_cart_item"), self.pool.connection() as conn:
            conn.execute("INSERT INTO cart_items (cart_id, sku, qty) VALUES (%s, %s, %s) "
                         "ON CONFLICT (cart_id, sku) DO UPDATE SET qty = cart_items.qty + EXCLUDED.qty",
                         (cart_id, sku, qty))

    def delete(self, cart_id: str) -> None:
        with self.metrics.query("delete_cart"), self.pool.connection() as conn, conn.transaction():
            conn.execute("DELETE FROM cart_items WHERE cart_id = %s", (cart_id,))
            conn.execute("DELETE FROM carts WHERE id = %s", (cart_id,))
