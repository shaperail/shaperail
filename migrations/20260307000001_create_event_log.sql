-- Event log table: append-only, stores all emitted events for audit + replay
CREATE TABLE IF NOT EXISTS steel_event_log (
    event_id   TEXT PRIMARY KEY,
    event      TEXT NOT NULL,
    resource   TEXT NOT NULL,
    action     TEXT NOT NULL,
    data       JSONB NOT NULL DEFAULT '{}',
    timestamp  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_steel_event_log_resource ON steel_event_log (resource);
CREATE INDEX IF NOT EXISTS idx_steel_event_log_event ON steel_event_log (event);
CREATE INDEX IF NOT EXISTS idx_steel_event_log_timestamp ON steel_event_log (timestamp DESC);

-- Webhook delivery log table: tracks every outbound webhook delivery attempt
CREATE TABLE IF NOT EXISTS steel_webhook_delivery_log (
    delivery_id TEXT PRIMARY KEY,
    event_id    TEXT NOT NULL,
    url         TEXT NOT NULL,
    status_code INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'pending',
    latency_ms  BIGINT NOT NULL DEFAULT 0,
    error       TEXT,
    attempt     INTEGER NOT NULL DEFAULT 1,
    timestamp   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_steel_webhook_delivery_event_id ON steel_webhook_delivery_log (event_id);
CREATE INDEX IF NOT EXISTS idx_steel_webhook_delivery_timestamp ON steel_webhook_delivery_log (timestamp DESC);
