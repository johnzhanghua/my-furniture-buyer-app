-- Buyers. Money is always integer cents; timestamps are RFC 3339 UTC strings.
CREATE TABLE users (
    id            TEXT    PRIMARY KEY NOT NULL,
    email         TEXT    NOT NULL UNIQUE,
    display_name  TEXT    NOT NULL,
    password_hash TEXT    NOT NULL,
    budget_cents  INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT    NOT NULL
);

CREATE TABLE products (
    id          TEXT    PRIMARY KEY NOT NULL,
    sku         TEXT    NOT NULL UNIQUE,
    name        TEXT    NOT NULL,
    description TEXT    NOT NULL,
    category    TEXT    NOT NULL,
    price_cents INTEGER NOT NULL,
    stock       INTEGER NOT NULL DEFAULT 0,
    image_url   TEXT    NOT NULL DEFAULT ''
);

CREATE TABLE orders (
    id          TEXT    PRIMARY KEY NOT NULL,
    user_id     TEXT    NOT NULL REFERENCES users (id),
    total_cents INTEGER NOT NULL,
    status      TEXT    NOT NULL DEFAULT 'placed',
    created_at  TEXT    NOT NULL
);

-- unit_price_cents is a snapshot: historical orders must survive price changes.
CREATE TABLE order_items (
    id               TEXT    PRIMARY KEY NOT NULL,
    order_id         TEXT    NOT NULL REFERENCES orders (id) ON DELETE CASCADE,
    product_id       TEXT    NOT NULL REFERENCES products (id),
    quantity         INTEGER NOT NULL,
    unit_price_cents INTEGER NOT NULL
);

CREATE INDEX idx_orders_user       ON orders (user_id, created_at DESC);
CREATE INDEX idx_order_items_order ON order_items (order_id);
CREATE INDEX idx_products_category ON products (category);
