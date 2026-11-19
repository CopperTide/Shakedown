CREATE TABLE products (
    sku          text PRIMARY KEY,
    name         text NOT NULL,
    list_price   numeric(12, 2) NOT NULL CHECK (list_price >= 0),
    discount_pct integer NOT NULL DEFAULT 0 CHECK (discount_pct BETWEEN 0 AND 90),
    currency     char(3) NOT NULL DEFAULT 'USD',
    on_hand      integer NOT NULL CHECK (on_hand >= 0)
);

CREATE TABLE reservations (
    id         text PRIMARY KEY,
    status     text NOT NULL CHECK (status IN ('held', 'committed')),
    expires_at bigint NOT NULL
);

CREATE INDEX reservations_held_idx ON reservations (expires_at) WHERE status = 'held';

CREATE TABLE reservation_lines (
    reservation_id text NOT NULL REFERENCES reservations (id),
    sku            text NOT NULL REFERENCES products (sku),
    qty            integer NOT NULL CHECK (qty > 0),
    unit_price     numeric(12, 2) NOT NULL,
    PRIMARY KEY (reservation_id, sku)
);
