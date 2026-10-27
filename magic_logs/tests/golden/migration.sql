-- magic_logs: "telemetry"."events", a hypertable of 1 day chunks compressed after 2 days, counted by hour, by day, and by day and unit_id.

SELECT create_hypertable('"telemetry"."events"', by_range('at', INTERVAL '1 day'),
                         create_default_indexes => false, migrate_data => true);

CREATE INDEX "events_at_id_idx" ON "telemetry"."events" ("at" DESC, "id" DESC);
CREATE INDEX "events_unit_id_at_idx" ON "telemetry"."events" ("unit_id", "at" DESC, "id" DESC);
CREATE INDEX "events_kind_at_idx" ON "telemetry"."events" ("kind", "at" DESC, "id" DESC);

ALTER TABLE "telemetry"."events" SET (
    timescaledb.enable_columnstore,
    timescaledb.segmentby = '"unit_id"',
    timescaledb.orderby = '"at" DESC, "id" DESC'
);
CALL add_columnstore_policy('"telemetry"."events"', after => INTERVAL '2 days');

CREATE MATERIALIZED VIEW "telemetry"."events_hourly"
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT time_bucket(INTERVAL '1 hour', "at") AS bucket, "tenant_id", "zone_id", "severity", "kind", count(*) AS events
  FROM "telemetry"."events"
 GROUP BY 1, "tenant_id", "zone_id", "severity", "kind"
WITH NO DATA;

CREATE MATERIALIZED VIEW "telemetry"."events_daily"
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT time_bucket(INTERVAL '1 day', bucket) AS bucket, "tenant_id", "zone_id", "severity", "kind", sum(events) AS events
  FROM "telemetry"."events_hourly"
 GROUP BY 1, "tenant_id", "zone_id", "severity", "kind"
WITH NO DATA;

CREATE MATERIALIZED VIEW "telemetry"."events_daily_unit"
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT time_bucket(INTERVAL '1 day', "at") AS bucket, "tenant_id", "unit_id", "zone_id", "severity", "kind",
       count(*) AS events, min("at") AS first_at, max("at") AS last_at
  FROM "telemetry"."events"
 GROUP BY 1, "tenant_id", "unit_id", "zone_id", "severity", "kind"
WITH NO DATA;

SELECT add_continuous_aggregate_policy('"telemetry"."events_hourly"', start_offset => NULL, end_offset => INTERVAL '1 hour', schedule_interval => INTERVAL '5 minutes');
SELECT add_continuous_aggregate_policy('"telemetry"."events_daily"', start_offset => NULL, end_offset => INTERVAL '1 day', schedule_interval => INTERVAL '1 hour');
SELECT add_continuous_aggregate_policy('"telemetry"."events_daily_unit"', start_offset => NULL, end_offset => INTERVAL '1 hour', schedule_interval => INTERVAL '5 minutes');

SELECT setup_governed_view('api', 'events', 'telemetry', 'events', 'zone_id');
SELECT setup_governed_view('api', 'events_hourly', 'telemetry', 'events_hourly', 'zone_id');
SELECT setup_governed_view('api', 'events_daily', 'telemetry', 'events_daily', 'zone_id');
SELECT setup_governed_view('api', 'events_daily_unit', 'telemetry', 'events_daily_unit', 'zone_id');
