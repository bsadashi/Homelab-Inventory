-- BLE / UWB physical-asset tracker bindings.
--
-- Each row binds a single tracker (AirTag, Galaxy SmartTag, Tile,
-- generic BLE beacon, …) to one inventory item. `provider` and
-- `provider_id` together uniquely identify a tracker; sync logic
-- lives in `src/trackers/<provider>.rs` and is opt-in per
-- deployment.

CREATE TABLE IF NOT EXISTS trackers (
    id              TEXT PRIMARY KEY,
    item_id         TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    -- 'airtag' | 'smarttag' | 'tile' | 'ble-beacon' | 'ruuvi' | …
    -- Implementation-defined; the audit log records whatever string
    -- the operator chose so swapping providers stays auditable.
    provider        TEXT NOT NULL,
    -- Provider-side opaque id (Apple's hashed AirTag id, the
    -- SmartThings device UUID, the Tile id, the BLE MAC, …).
    provider_id     TEXT NOT NULL,
    -- Optional human label, e.g. "rack key" or "multimeter".
    label           TEXT,
    -- Most recent reported location. Populated by the sync workers;
    -- `last_seen_at` tells the dashboard whether the position is
    -- fresh enough to act on.
    last_seen_lat   REAL,
    last_seen_lng   REAL,
    last_seen_at    TEXT,
    -- Free-text human-readable location ("RACK-A · U6") so a sync
    -- worker can write whatever the upstream service hands it
    -- without reverse-geocoding.
    last_seen_label TEXT,
    -- Best-effort battery percent from the upstream service.
    battery_pct     INTEGER,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(provider, provider_id)
);

CREATE INDEX IF NOT EXISTS idx_trackers_item ON trackers(item_id);
CREATE INDEX IF NOT EXISTS idx_trackers_provider ON trackers(provider);
