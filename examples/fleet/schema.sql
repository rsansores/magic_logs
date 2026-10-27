-- The fleet's table, as a host would have it before magic_logs: the rows
-- in a schema only the fleet's own work reads, a primary key holding the
-- time (a hypertable's unique keys hold its time column).
CREATE SCHEMA IF NOT EXISTS fleet;
CREATE SCHEMA IF NOT EXISTS api;

CREATE TABLE IF NOT EXISTS fleet.events (
    id      uuid        NOT NULL,
    at      timestamptz NOT NULL,
    level   text        NOT NULL CHECK (level IN ('info', 'warning', 'error', 'critical')),
    kind    text        NOT NULL,
    zone    text        NOT NULL,
    unit    text        NOT NULL,
    message text        NOT NULL,
    PRIMARY KEY (id, at)
);
