-- The shop's history before the world starts: the catalog, :rows past orders (one line each, spread over the
-- last 400 days, with the notes customer service wrote), and committed reservations for the first thousand.
-- Run with psql -v rows=N once the services have migrated; a database that already has orders is left alone.
\set ON_ERROR_STOP on

CREATE TEMP TABLE catalog (LIKE inventory.products);
\copy catalog FROM '/seed/catalog.csv' WITH (FORMAT csv, HEADER true)
INSERT INTO inventory.products SELECT * FROM catalog ON CONFLICT (sku) DO NOTHING;

SELECT NOT EXISTS (SELECT 1 FROM orders.orders) AS empty \gset
\if :empty
BEGIN;

CREATE TEMP TABLE history AS
SELECT g AS id,
       'c-' || lpad((1 + (g::bigint * 7919) % 20000)::text, 5, '0') AS customer,
       'r-' || lpad(g::text, 10, '0') AS reservation_id,
       p.sku, p.list_price AS unit_price, 1 + g % 3 AS qty,
       (ARRAY['delivered', 'delivered', 'delivered', 'shipped', 'cancelled', 'refunded', 'placed'])[1 + g % 7] AS status,
       now() - make_interval(secs => (g::bigint * 34560000 / GREATEST(:rows, 1))) AS created_at
FROM generate_series(1, :rows) AS g
JOIN (SELECT sku, list_price, row_number() OVER (ORDER BY sku) - 1 AS n FROM inventory.products) AS p
  ON p.n = g % (SELECT count(*) FROM inventory.products);

INSERT INTO orders.orders (id, customer, reservation_id, status, total, notes, created_at)
SELECT id, customer, reservation_id, status, unit_price * qty,
       CASE status
           WHEN 'cancelled' THEN 'Cancelled: ' || (ARRAY['customer changed their mind', 'payment declined', 'ordered twice'])[1 + id % 3]
           WHEN 'refunded' THEN 'Refund requested: ' || (ARRAY['arrived damaged', 'wrong colour', 'late delivery', 'not as described'])[1 + id % 4]
           ELSE (ARRAY['', '', '', 'Gift wrap, please', 'Leave with the neighbour', 'Call before delivery'])[1 + id % 6]
       END,
       created_at
FROM history ORDER BY id;

INSERT INTO orders.order_lines (order_id, sku, qty, unit_price, amount)
SELECT id, sku, qty, unit_price, unit_price * qty FROM history ORDER BY id;

SELECT setval(pg_get_serial_sequence('orders.orders', 'id'), (SELECT max(id) FROM orders.orders)) AS last_id \gset

INSERT INTO inventory.reservations (id, status, expires_at)
SELECT reservation_id, 'committed', extract(epoch FROM created_at)::bigint + 900 FROM history WHERE id <= 1000;
INSERT INTO inventory.reservation_lines (reservation_id, sku, qty, unit_price)
SELECT reservation_id, sku, qty, unit_price FROM history WHERE id <= 1000;

COMMIT;
\endif

ANALYZE;
