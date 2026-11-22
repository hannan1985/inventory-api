CREATE TABLE stock_movements (
    id         BIGSERIAL PRIMARY KEY,
    sku        TEXT NOT NULL REFERENCES items (sku) ON DELETE RESTRICT,
    kind       TEXT NOT NULL CHECK (kind IN ('receive', 'issue')),
    quantity   INTEGER NOT NULL CHECK (quantity > 0),
    note       TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_stock_movements_sku_created ON stock_movements (sku, created_at);
