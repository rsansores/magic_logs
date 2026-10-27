-- page
SELECT * FROM "api"."events" WHERE "at" >= $1 AND "at" < $2 ORDER BY "at" DESC, "id" DESC LIMIT $3;

-- page after a cursor, narrowed and searched
SELECT * FROM "api"."events" WHERE "at" >= $1 AND "at" < $2 AND "kind" = ANY($3::text[]) AND (f_unaccent("description"::text) ILIKE f_unaccent('%' || $4 || '%')) AND ("at", "id") < ($5, $6::uuid) ORDER BY "at" DESC, "id" DESC LIMIT $7;

-- histogram by day: the daily counts
SELECT time_bucket($1, bucket) AS bucket, "severity"::text AS level, sum(events)::bigint AS count FROM "api"."events_daily" WHERE bucket >= time_bucket(INTERVAL '1 day', $2) AND bucket < $3 GROUP BY 1, 2 ORDER BY 1, 2;

-- histogram by hour, narrowed to counted fields: the hourly counts
SELECT time_bucket($1, bucket) AS bucket, "severity"::text AS level, sum(events)::bigint AS count FROM "api"."events_hourly" WHERE bucket >= time_bucket(INTERVAL '1 hour', $2) AND bucket < $3 AND "kind" = ANY($4::text[]) AND "severity" = ANY($5::severity[]) GROUP BY 1, 2 ORDER BY 1, 2;

-- histogram by five minutes: the rows
SELECT time_bucket($1, "at") AS bucket, "severity"::text AS level, count(*) AS count FROM "api"."events" WHERE "at" >= $2 AND "at" < $3 GROUP BY 1, 2 ORDER BY 1, 2;

-- histogram narrowed to a field the counts do not keep: the rows
SELECT time_bucket($1, "at") AS bucket, "severity"::text AS level, count(*) AS count FROM "api"."events" WHERE "at" >= $2 AND "at" < $3 AND "operator_id" = ANY($4::uuid[]) GROUP BY 1, 2 ORDER BY 1, 2;

-- facets over a week, each under the others' filters
(SELECT 'severity' AS field, "severity"::text AS value, count(*) AS count FROM "api"."events" WHERE "at" >= $1 AND "at" < $2 AND "kind" = ANY($3::text[]) AND "unit_id" = ANY($4::uuid[]) GROUP BY 2) UNION ALL (SELECT 'kind' AS field, "kind"::text AS value, count(*) AS count FROM "api"."events" WHERE "at" >= $5 AND "at" < $6 AND "unit_id" = ANY($7::uuid[]) GROUP BY 2) UNION ALL (SELECT 'unit_id' AS field, "unit_id"::text AS value, sum(events)::bigint AS count FROM "api"."events_daily_unit" WHERE bucket >= time_bucket(INTERVAL '1 day', $8) AND bucket < $9 AND "kind" = ANY($10::text[]) GROUP BY 2);

-- facets over six hours: the hourly counts
(SELECT 'severity' AS field, "severity"::text AS value, sum(events)::bigint AS count FROM "api"."events_hourly" WHERE bucket >= time_bucket(INTERVAL '1 hour', $1) AND bucket < $2 GROUP BY 2) UNION ALL (SELECT 'kind' AS field, "kind"::text AS value, sum(events)::bigint AS count FROM "api"."events_hourly" WHERE bucket >= time_bucket(INTERVAL '1 hour', $3) AND bucket < $4 GROUP BY 2);

-- grouped: the counts per day and top
SELECT "unit_id"::text AS top, "kind"::text AS value, sum(events)::bigint AS count, min(first_at) AS first_at, max(last_at) AS last_at FROM "api"."events_daily_unit" WHERE bucket >= time_bucket(INTERVAL '1 day', $1) AND bucket < $2 GROUP BY 1, 2 ORDER BY 3 DESC, 1, 2 LIMIT $3;

-- grouped, searched: the rows
SELECT "unit_id"::text AS top, "kind"::text AS value, count(*) AS count, min("at") AS first_at, max("at") AS last_at FROM "api"."events" WHERE "at" >= $1 AND "at" < $2 AND (f_unaccent("description"::text) ILIKE f_unaccent('%' || $3 || '%')) GROUP BY 1, 2 ORDER BY 3 DESC, 1, 2 LIMIT $4;
