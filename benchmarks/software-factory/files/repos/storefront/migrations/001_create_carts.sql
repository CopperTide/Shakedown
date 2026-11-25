CREATE TABLE carts (
    id         text PRIMARY KEY,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE cart_items (
    cart_id text NOT NULL REFERENCES carts (id),
    sku     text NOT NULL,
    qty     integer NOT NULL CHECK (qty > 0),
    PRIMARY KEY (cart_id, sku)
);
