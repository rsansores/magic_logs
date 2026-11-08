-- magic_logs: "telemetry"."pings", a hypertable of 1 day chunks compressed after 2 days, stored as its rows alone.

DO $$
DECLARE
    v text := (SELECT extversion FROM pg_extension WHERE extname = 'timescaledb');
    n int[] := (regexp_match(v, '^(\d+)\.(\d+)'))::int[];
BEGIN
    IF n < ARRAY[2, 21] OR (n >= ARRAY[2, 22] AND n < ARRAY[2, 24]) THEN
        RAISE EXCEPTION 'magic_logs needs TimescaleDB 2.21, or 2.24 or later; this database has %', v;
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

SELECT setup_governed_view('public', 'pings', 'telemetry', 'pings', NULL);
