-- RACKLOG initial schema.
--
-- Targets SQLite. JSON-typed columns are stored as TEXT and
-- ON DELETE CASCADE is used sparingly so audit references survive
-- deletions. The schema is *largely* portable to Postgres but the
-- runtime code is not — see docs/postgres.md before assuming a
-- DATABASE_URL swap will work.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS locations (
    id          TEXT PRIMARY KEY,
    code        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    type        TEXT NOT NULL,
    parent      TEXT REFERENCES locations(id) ON DELETE SET NULL,
    bins        TEXT NOT NULL DEFAULT '[]',  -- JSON array of bin codes
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_locations_parent ON locations(parent);

CREATE TABLE IF NOT EXISTS suppliers (
    id           TEXT PRIMARY KEY,
    code         TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    contact      TEXT,
    lead_time    INTEGER,
    rating       REAL,
    total_spend  REAL NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS items (
    id           TEXT PRIMARY KEY,
    sku          TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    category     TEXT NOT NULL,
    brand        TEXT,
    supplier_id  TEXT REFERENCES suppliers(id) ON DELETE SET NULL,
    cost         REAL NOT NULL DEFAULT 0,
    price        REAL NOT NULL DEFAULT 0,
    unit         TEXT NOT NULL DEFAULT 'ea',
    min_qty      INTEGER NOT NULL DEFAULT 0,
    max_qty      INTEGER NOT NULL DEFAULT 0,
    qty          INTEGER NOT NULL DEFAULT 0,
    allocated    INTEGER NOT NULL DEFAULT 0,
    barcode      TEXT,
    variants     TEXT,                       -- JSON
    lots         TEXT,                       -- JSON
    tags         TEXT NOT NULL DEFAULT '[]', -- JSON array
    img          TEXT,
    updated      TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_items_category ON items(category);
CREATE INDEX IF NOT EXISTS idx_items_supplier ON items(supplier_id);
CREATE INDEX IF NOT EXISTS idx_items_barcode  ON items(barcode);

CREATE TABLE IF NOT EXISTS item_stock (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id      TEXT NOT NULL REFERENCES items(id)     ON DELETE CASCADE,
    location_id  TEXT NOT NULL REFERENCES locations(id) ON DELETE RESTRICT,
    bin          TEXT,
    qty          INTEGER NOT NULL DEFAULT 0,
    serials      TEXT,    -- JSON array
    UNIQUE(item_id, location_id, bin)
);
CREATE INDEX IF NOT EXISTS idx_item_stock_item     ON item_stock(item_id);
CREATE INDEX IF NOT EXISTS idx_item_stock_location ON item_stock(location_id);

CREATE TABLE IF NOT EXISTS purchase_orders (
    id           TEXT PRIMARY KEY,
    supplier_id  TEXT REFERENCES suppliers(id) ON DELETE SET NULL,
    status       TEXT NOT NULL,
    created      TEXT NOT NULL,
    expected     TEXT,
    received     TEXT,
    total        REAL NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_po_status ON purchase_orders(status);

CREATE TABLE IF NOT EXISTS purchase_order_lines (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    po_id  TEXT NOT NULL REFERENCES purchase_orders(id) ON DELETE CASCADE,
    sku    TEXT NOT NULL,
    qty    INTEGER NOT NULL,
    cost   REAL NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_pol_po ON purchase_order_lines(po_id);

CREATE TABLE IF NOT EXISTS sales_orders (
    id          TEXT PRIMARY KEY,
    project     TEXT,
    status      TEXT NOT NULL,
    priority    TEXT,
    created     TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_so_status ON sales_orders(status);

CREATE TABLE IF NOT EXISTS sales_order_lines (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    so_id  TEXT NOT NULL REFERENCES sales_orders(id) ON DELETE CASCADE,
    sku    TEXT NOT NULL,
    qty    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sol_so ON sales_order_lines(so_id);

CREATE TABLE IF NOT EXISTS transfers (
    id          TEXT PRIMARY KEY,
    from_loc    TEXT REFERENCES locations(id) ON DELETE SET NULL,
    to_loc      TEXT REFERENCES locations(id) ON DELETE SET NULL,
    date        TEXT NOT NULL,
    status      TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS transfer_lines (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    transfer_id   TEXT NOT NULL REFERENCES transfers(id) ON DELETE CASCADE,
    sku           TEXT NOT NULL,
    qty           INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS counts (
    id           TEXT PRIMARY KEY,
    location_id  TEXT REFERENCES locations(id) ON DELETE SET NULL,
    date         TEXT NOT NULL,
    status       TEXT NOT NULL,
    counted      INTEGER NOT NULL DEFAULT 0,
    variance     INTEGER NOT NULL DEFAULT 0,
    by_user      TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Tamper-evident activity log. Each row's hash chains the previous one;
-- breaking the chain (or rewriting history) is detectable on replay.
CREATE TABLE IF NOT EXISTS activity_log (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    ts           TEXT NOT NULL,
    user_name    TEXT NOT NULL,
    type         TEXT NOT NULL,
    ref          TEXT,
    description  TEXT NOT NULL,
    payload      TEXT,             -- optional JSON context
    prev_hash    TEXT,
    hash         TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_activity_ts   ON activity_log(ts);
CREATE INDEX IF NOT EXISTS idx_activity_type ON activity_log(type);
