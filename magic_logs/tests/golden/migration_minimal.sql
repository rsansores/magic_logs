-- magic_logs: "telemetry"."pings", a hypertable of 1 day chunks compressed after 2 days, counted by hour and by day.

DO $$
DECLARE
    v text := (SELECT extversion FROM pg_extension WHERE extname = 'timescaledb');
BEGIN
    IF (regexp_match(v, '^(\d+)\.(\d+)'))::int[] < ARRAY[2, 21] OR v = '2.22.0' THEN
        RAISE EXCEPTION 'magic_logs needs TimescaleDB 2.21 or later, but not 2.22.0; this database has %', v;
    END IF;
END
$$;

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
 GROUP BY 1
WITH NO DATA;

CREATE MATERIALIZED VIEW "telemetry"."pings_daily"
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT time_bucket(INTERVAL '1 day', bucket) AS bucket, sum(events) AS events
  FROM "telemetry"."pings_hourly"
 GROUP BY 1
WITH NO DATA;

SELECT add_continuous_aggregate_policy('"telemetry"."pings_hourly"', start_offset => NULL, end_offset => INTERVAL '1 hour', schedule_interval => INTERVAL '5 minutes', refresh_newest_first => false);
SELECT add_continuous_aggregate_policy('"telemetry"."pings_daily"', start_offset => NULL, end_offset => INTERVAL '1 day', schedule_interval => INTERVAL '1 hour', refresh_newest_first => false);

SELECT set_chunk_time_interval('"telemetry"."pings_hourly"', INTERVAL '1 day');
ALTER MATERIALIZED VIEW "telemetry"."pings_hourly" SET (
    timescaledb.enable_columnstore = true,
    timescaledb.segmentby = '',
    timescaledb.orderby = 'bucket'
);
CALL add_columnstore_policy('"telemetry"."pings_hourly"', after => INTERVAL '2 days');
SELECT set_chunk_time_interval('"telemetry"."pings_daily"', INTERVAL '1 day');
ALTER MATERIALIZED VIEW "telemetry"."pings_daily" SET (
    timescaledb.enable_columnstore = true,
    timescaledb.segmentby = '',
    timescaledb.orderby = 'bucket'
);
CALL add_columnstore_policy('"telemetry"."pings_daily"', after => INTERVAL '2 days');

SELECT setup_governed_view('public', 'pings', 'telemetry', 'pings', NULL);
SELECT setup_governed_view('public', 'pings_hourly', 'telemetry', 'pings_hourly', NULL);
SELECT setup_governed_view('public', 'pings_daily', 'telemetry', 'pings_daily', NULL);
