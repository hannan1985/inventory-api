CREATE TABLE items (
    sku        TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    quantity   INTEGER NOT NULL DEFAULT 0 CHECK (quantity >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
