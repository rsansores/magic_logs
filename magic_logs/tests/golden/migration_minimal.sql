-- magic_logs: "telemetry"."pings", a hypertable of 1 day chunks compressed after 2 days, counted by hour and by day.

SELECT create_hypertable('"telemetry"."pings"', by_range('at', INTERVAL '1 day'),
                         create_default_indexes => false, migrate_data => true);

CREATE INDEX "pings_at_id_idx" ON "telemetry"."pings" ("at" DESC, "id" DESC);

ALTER TABLE "telemetry"."pings" SET (
    timescaledb.enable_columnstore,
    timescaledb.segmentby = '',
    timescaledb.orderby = '"at" DESC, "id" DESC'
);
CALL add_columnstore_policy('"telemetry"."pings"', after => INTERVAL '2 days');

CREATE MATERIALIZED VIEW "telemetry"."pings_hourly"
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT time_bucket(INTERVAL '1 hour', "at") AS bucket, count(*) AS events
  FROM "telemetry"."pings"
 GROUP BY bucket
WITH NO DATA;

CREATE MATERIALIZED VIEW "telemetry"."pings_daily"
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT time_bucket(INTERVAL '1 day', bucket) AS bucket, sum(events) AS events
  FROM "telemetry"."pings_hourly"
 GROUP BY 1
WITH NO DATA;

SELECT add_continuous_aggregate_policy('"telemetry"."pings_hourly"', start_offset => NULL, end_offset => INTERVAL '1 hour', schedule_interval => INTERVAL '5 minutes');
SELECT add_continuous_aggregate_policy('"telemetry"."pings_daily"', start_offset => NULL, end_offset => INTERVAL '1 day', schedule_interval => INTERVAL '1 hour');

SELECT setup_governed_view('public', 'pings', 'telemetry', 'pings', NULL);
SELECT setup_governed_view('public', 'pings_hourly', 'telemetry', 'pings_hourly', NULL);
SELECT setup_governed_view('public', 'pings_daily', 'telemetry', 'pings_daily', NULL);
