-- The fleet's table, as a host would have it before magic_logs: the rows
-- in a schema only the fleet's own work reads, a primary key holding the
-- time (a hypertable's unique keys hold its time column).
CREATE EXTENSION IF NOT EXISTS timescaledb;
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
-- Added after the first release: a database made before it gains the column
-- (and its view, made again at start, the column with it).
ALTER TABLE fleet.events ADD COLUMN IF NOT EXISTS odometer bigint;

-- Where each truck is, every few seconds: kept and read, never counted.
CREATE TABLE IF NOT EXISTS fleet.positions (
    id    uuid             NOT NULL,
    at    timestamptz      NOT NULL,
    unit  text             NOT NULL,
    zone  text             NOT NULL,
    lat   double precision NOT NULL,
    lon   double precision NOT NULL,
    speed integer          NOT NULL,
    PRIMARY KEY (id, at)
);
