-- page
SELECT * FROM "api"."events" WHERE "at" >= $1 AND "at" < $2 ORDER BY "at" DESC, "id" DESC LIMIT $3;

-- page after a cursor, narrowed and searched
SELECT * FROM "api"."events" WHERE "at" >= $1 AND "at" < $2 AND "kind" = ANY((SELECT $3::text[])::text[]) AND (f_unaccent("description"::text) ILIKE '%' || replace(replace(replace(f_unaccent($4), '\', '\\'), '%', '\%'), '_', '\_') || '%') AND "at" <= $5 AND ("at", "id") < ($6, $7::uuid) ORDER BY "at" DESC, "id" DESC LIMIT $8;

-- histogram by day: the daily counts
SELECT time_bucket($1, bucket) AS bucket, "severity"::text AS level, sum(events)::bigint AS count FROM "api"."events_daily" WHERE bucket >= time_bucket($2, $3) AND bucket < time_bucket($4, $5 - INTERVAL '1 microsecond') + $6 GROUP BY 1, 2 ORDER BY 1, 2;

-- histogram by hour, narrowed to counted fields: the hourly counts
SELECT time_bucket($1, bucket) AS bucket, "severity"::text AS level, sum(events)::bigint AS count FROM "api"."events_hourly" WHERE bucket >= time_bucket($2, $3) AND bucket < time_bucket($4, $5 - INTERVAL '1 microsecond') + $6 AND "kind" = ANY((SELECT $7::text[])::text[]) AND "severity" = ANY((SELECT $8::severity[])::severity[]) GROUP BY 1, 2 ORDER BY 1, 2;

-- histogram by day narrowed to one top: the counts per day and top
SELECT time_bucket($1, bucket) AS bucket, "severity"::text AS level, sum(events)::bigint AS count FROM "api"."events_daily_unit" WHERE bucket >= time_bucket($2, $3) AND bucket < time_bucket($4, $5 - INTERVAL '1 microsecond') + $6 AND "unit_id" = ANY((SELECT $7::uuid[])::uuid[]) GROUP BY 1, 2 ORDER BY 1, 2;

-- histogram by five minutes: the rows
SELECT time_bucket($1, "at") AS bucket, "severity"::text AS level, count(*) AS count FROM "api"."events" WHERE "at" >= time_bucket($2, $3) AND "at" < time_bucket($4, $5 - INTERVAL '1 microsecond') + $6 GROUP BY 1, 2 ORDER BY 1, 2;

-- histogram narrowed to a field the counts do not keep: the rows
SELECT time_bucket($1, "at") AS bucket, "severity"::text AS level, count(*) AS count FROM "api"."events" WHERE "at" >= time_bucket($2, $3) AND "at" < time_bucket($4, $5 - INTERVAL '1 microsecond') + $6 AND "operator_id" = ANY((SELECT $7::uuid[])::uuid[]) GROUP BY 1, 2 ORDER BY 1, 2;

-- facets over a week, each under the others' filters
(SELECT 'severity' AS field, value, sum(n)::bigint AS count FROM (SELECT "severity"::text AS value, sum(events)::bigint AS n FROM "api"."events_daily_unit" WHERE bucket >= $1 AND bucket < $2 AND "kind" = ANY((SELECT $3::text[])::text[]) AND "unit_id" = ANY((SELECT $4::uuid[])::uuid[]) GROUP BY 1) AS parts GROUP BY 2) UNION ALL (SELECT 'kind' AS field, value, sum(n)::bigint AS count FROM (SELECT "kind"::text AS value, sum(events)::bigint AS n FROM "api"."events_daily_unit" WHERE bucket >= $5 AND bucket < $6 AND "unit_id" = ANY((SELECT $7::uuid[])::uuid[]) GROUP BY 1) AS parts GROUP BY 2) UNION ALL (SELECT 'unit_id' AS field, value, sum(n)::bigint AS count FROM (SELECT "unit_id"::text AS value, sum(events)::bigint AS n FROM "api"."events_daily_unit" WHERE bucket >= $8 AND bucket < $9 AND "kind" = ANY((SELECT $10::text[])::text[]) GROUP BY 1) AS parts GROUP BY 2);

-- facets over a window off the hour: its minutes from the rows, its hours and days from the counts
(SELECT 'severity' AS field, value, sum(n)::bigint AS count FROM (SELECT "severity"::text AS value, count(*)::bigint AS n FROM "api"."events" WHERE "at" >= $1 AND "at" < $2 GROUP BY 1 UNION ALL SELECT "severity"::text AS value, sum(events)::bigint AS n FROM "api"."events_hourly" WHERE bucket >= $3 AND bucket < $4 GROUP BY 1 UNION ALL SELECT "severity"::text AS value, sum(events)::bigint AS n FROM "api"."events_daily" WHERE bucket >= $5 AND bucket < $6 GROUP BY 1 UNION ALL SELECT "severity"::text AS value, sum(events)::bigint AS n FROM "api"."events_hourly" WHERE bucket >= $7 AND bucket < $8 GROUP BY 1 UNION ALL SELECT "severity"::text AS value, count(*)::bigint AS n FROM "api"."events" WHERE "at" >= $9 AND "at" < $10 GROUP BY 1) AS parts GROUP BY 2);

-- facets over six hours: the hourly counts
(SELECT 'severity' AS field, value, sum(n)::bigint AS count FROM (SELECT "severity"::text AS value, sum(events)::bigint AS n FROM "api"."events_hourly" WHERE bucket >= $1 AND bucket < $2 GROUP BY 1) AS parts GROUP BY 2) UNION ALL (SELECT 'kind' AS field, value, sum(n)::bigint AS count FROM (SELECT "kind"::text AS value, sum(events)::bigint AS n FROM "api"."events_hourly" WHERE bucket >= $3 AND bucket < $4 GROUP BY 1) AS parts GROUP BY 2);

-- grouped: the counts per day and top
SELECT top, value, sum(n)::bigint AS count, min(first) AS first_at, max(last) AS last_at FROM (SELECT "unit_id"::text AS top, "kind"::text AS value, sum(events)::bigint AS n, min(first_at) AS first, max(last_at) AS last FROM "api"."events_daily_unit" WHERE bucket >= $1 AND bucket < $2 GROUP BY 1, 2) AS parts GROUP BY 1, 2 ORDER BY 3 DESC, 1, 2 LIMIT $3;

-- grouped, searched: the rows
SELECT top, value, sum(n)::bigint AS count, min(first) AS first_at, max(last) AS last_at FROM (SELECT "unit_id"::text AS top, "kind"::text AS value, count(*)::bigint AS n, min("at") AS first, max("at") AS last FROM "api"."events" WHERE "at" >= $1 AND "at" < $2 AND (f_unaccent("description"::text) ILIKE '%' || replace(replace(replace(f_unaccent($3), '\', '\\'), '%', '\%'), '_', '\_') || '%') GROUP BY 1, 2) AS parts GROUP BY 1, 2 ORDER BY 3 DESC, 1, 2 LIMIT $4;
