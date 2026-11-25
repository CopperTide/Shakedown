CREATE TABLE orders (
    id             bigserial PRIMARY KEY,
    customer       text NOT NULL,
    reservation_id text NOT NULL UNIQUE,
    status         text NOT NULL DEFAULT 'placed',
    total          numeric(12, 2) NOT NULL,
    notes          text NOT NULL DEFAULT '',
    created_at     timestamptz NOT NULL
);

CREATE INDEX orders_customer_idx ON orders (customer, id DESC);

CREATE TABLE order_lines (
    order_id   bigint NOT NULL REFERENCES orders (id),
    sku        text NOT NULL,
    qty        integer NOT NULL CHECK (qty > 0),
    unit_price numeric(12, 2) NOT NULL,
    amount     numeric(12, 2) NOT NULL,
    PRIMARY KEY (order_id, sku)
);
