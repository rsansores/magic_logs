//! The migration on a real TimescaleDB, and every question answered alike
//! from the counts and from the rows. Needs DATABASE_URL to a server with
//! the timescaledb extension preloaded.

use chrono::{DateTime, Duration, TimeZone, Utc};
use magic_logs::{
    facets, grouped, histogram, migration, page, Bucket, Cursor, FacetCount, Filter, Governed,
    Group, MagicLog, Tier,
};
use sqlx::PgPool;
use uuid::Uuid;

#[allow(dead_code)]
#[derive(MagicLog, sqlx::FromRow, Debug, Clone)]
#[log(
    name = "events",
    schema = "private",
    chunk = "1 day",
    compress_after = "2 days",
    segment_by = "unit_id",
    search_with = "lower"
)]
struct Event {
    #[log(id)]
    id: Uuid,
    #[log(time)]
    at: DateTime<Utc>,
    #[log(tenant)]
    tenant_id: Uuid,
    #[log(scope)]
    zone_id: Uuid,
    #[log(level, sql = "level")]
    level: Level,
    #[log(facet, per_top, indexed)]
    kind: String,
    #[log(top)]
    unit_id: Uuid,
    #[log(filter)]
    operator: Option<String>,
    #[log(search)]
    description: String,
}

#[derive(sqlx::Type, Debug, Clone, Copy, PartialEq, Eq)]
#[sqlx(type_name = "level")]
enum Level {
    Info,
    Warning,
    Error,
}

const TENANT: Uuid = Uuid::from_u128(1);

fn day() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()
}

/// No authorization: every relation read as it is.
fn plain(g: &Governed) -> String {
    format!(
        "CREATE VIEW \"{}\".\"{}\" AS SELECT * FROM \"{}\".\"{}\";",
        g.view_schema, g.view, g.schema, g.relation
    )
}

async fn setup(pool: &PgPool) {
    sqlx::raw_sql(
        "CREATE EXTENSION IF NOT EXISTS timescaledb; \
         CREATE TYPE level AS ENUM ('Info', 'Warning', 'Error'); \
         CREATE SCHEMA private; \
         CREATE TABLE private.events (id uuid NOT NULL, at timestamptz NOT NULL, tenant_id uuid NOT NULL, \
             zone_id uuid NOT NULL, level level NOT NULL, kind text NOT NULL, unit_id uuid NOT NULL, \
             operator text, description text NOT NULL, PRIMARY KEY (id, at));",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::raw_sql(&migration(&Event::SPEC, plain))
        .execute(pool)
        .await
        .unwrap();
    // Four days, three units in two zones, every 10 minutes, a mix of kinds and levels.
    sqlx::raw_sql(&format!(
        "INSERT INTO private.events \
         SELECT gen_random_uuid(), '{start}'::timestamptz + n * interval '10 minutes', '{TENANT}', \
                ('00000000-0000-0000-0000-00000000000' || (1 + u % 2))::uuid, \
                (ARRAY['Info','Info','Warning','Error']::level[])[1 + (n + u) % 4], \
                (ARRAY['door','door','door','power','gps'])[1 + (n * 7 + u) % 5], \
                ('00000000-0000-0000-0000-0000000000a' || u)::uuid, \
                CASE WHEN n % 3 = 0 THEN 'ana' END, \
                'unit ' || u || ' said ' || CASE WHEN n % 10 = 0 THEN 'hola mundo' ELSE 'ok' END \
           FROM generate_series(0, 4 * 144 - 1) n, generate_series(1, 3) u;",
        start = day().to_rfc3339()
    ))
    .execute(pool)
    .await
    .unwrap();
    for tier in [Tier::Hourly, Tier::Daily, Tier::DailyTop] {
        sqlx::query(&format!(
            "CALL refresh_continuous_aggregate('private.{}', NULL, NULL)",
            Event::SPEC.relation(tier)
        ))
        .execute(pool)
        .await
        .unwrap();
    }
}

fn window(days: i64) -> Filter {
    Filter::new(day(), day() + Duration::days(days))
}

#[sqlx::test(migrations = false)]
async fn the_counts_answer_what_the_rows_answer(pool: PgPool) {
    setup(&pool).await;
    let spec = Event::SPEC;

    // Histogram: per hour from the hourly counts, per day from the daily
    // ones, against the rows themselves.
    for (filter, step) in [
        (window(1), Duration::hours(1)),
        (window(4), Duration::days(1)),
        (window(4).with("kind", ["door"]), Duration::days(1)),
        (
            window(4).with("level", ["Error", "Warning"]),
            Duration::hours(6),
        ),
    ] {
        let counted: Vec<Bucket> = histogram(&spec, &filter, step)
            .unwrap()
            .build_query_as()
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_ne!(
            magic_logs::counts_tier(&spec, &filter, Some(step)),
            Tier::Raw
        );
        let raw: Vec<Bucket> = histogram(&spec, &filter.clone().search(""), step)
            .unwrap()
            .build_query_as()
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(counted, raw, "{filter:?} by {step}");
        assert!(!counted.is_empty());
    }

    // Facets, each under the other filters but its own.
    let filter = window(4).with("kind", ["door"]).with("level", ["Error"]);
    let mut counted: Vec<FacetCount> =
        facets(&spec, &filter, &["kind", "level", "zone_id", "unit_id"])
            .unwrap()
            .build_query_as()
            .fetch_all(&pool)
            .await
            .unwrap();
    let mut raw: Vec<FacetCount> = facets(
        &spec,
        &filter.clone().search(""),
        &["kind", "level", "zone_id", "unit_id"],
    )
    .unwrap()
    .build_query_as()
    .fetch_all(&pool)
    .await
    .unwrap();
    counted.sort_by(|a, b| (&a.field, &a.value).cmp(&(&b.field, &b.value)));
    raw.sort_by(|a, b| (&a.field, &a.value).cmp(&(&b.field, &b.value)));
    assert_eq!(counted, raw);
    let kinds: Vec<&str> = counted
        .iter()
        .filter(|f| f.field == "kind")
        .filter_map(|f| f.value.as_deref())
        .collect();
    assert_eq!(
        kinds,
        ["door", "gps", "power"],
        "a facet is not narrowed by its own filter"
    );

    // Grouped: per unit and kind, with the first and the last.
    let counted: Vec<Group> = grouped(&spec, &window(4), "kind", 5)
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    let raw: Vec<Group> = grouped(&spec, &window(4).search(""), "kind", 5)
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(counted, raw);
    assert_eq!(counted.len(), 5);
}

#[sqlx::test(migrations = false)]
async fn a_page_follows_its_cursor_newest_first(pool: PgPool) {
    setup(&pool).await;
    let spec = Event::SPEC;
    let filter = window(4).with("kind", ["power"]).with("operator", ["ana"]);
    let first: Vec<Event> = page(&spec, &filter, None, 50)
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(first.len(), 50);
    let last = first.last().unwrap();
    let cursor = Cursor {
        time: last.at,
        id: last.id.to_string(),
    };
    let second: Vec<Event> = page(&spec, &filter, Some(&cursor), 50)
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(second
        .first()
        .is_some_and(|e| (e.at, e.id) < (last.at, last.id)));
    let all: Vec<(DateTime<Utc>, Uuid)> =
        first.iter().chain(&second).map(|e| (e.at, e.id)).collect();
    let mut sorted = all.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(all, sorted);
    assert!(first
        .iter()
        .chain(&second)
        .all(|e| e.kind == "power" && e.operator.as_deref() == Some("ana")));

    let found: Vec<Event> = page(&spec, &window(4).search("HOLA"), None, 1000)
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(found.len(), 58 * 3);
}

#[test]
fn a_filter_is_held_to_the_table() {
    let spec = Event::SPEC;
    assert_eq!(
        window(1).with("nope", ["x"]).check(&spec),
        Err(magic_logs::Error::UnknownField("nope".into()))
    );
    assert_eq!(
        Filter::new(day(), day()).check(&spec),
        Err(magic_logs::Error::Window)
    );
    assert_eq!(
        window(1)
            .with("kind", ["a"])
            .with("kind", ["b"])
            .check(&spec),
        Err(magic_logs::Error::FieldTwice("kind".into()))
    );
    let tier = |f: &Filter, step| magic_logs::counts_tier(&spec, f, step);
    assert_eq!(
        tier(
            &window(1).with("operator", ["ana"]),
            Some(Duration::hours(1))
        ),
        Tier::Raw
    );
    assert_eq!(tier(&window(1), Some(Duration::minutes(5))), Tier::Raw);
    assert_eq!(tier(&window(1), Some(Duration::minutes(90))), Tier::Raw);
    assert_eq!(tier(&window(1), Some(Duration::hours(1))), Tier::Hourly);
    assert_eq!(tier(&window(7), Some(Duration::hours(6))), Tier::Hourly);
    assert_eq!(tier(&window(7), Some(Duration::days(1))), Tier::Daily);
    assert_eq!(tier(&window(2), None), Tier::Hourly);
    assert_eq!(tier(&window(7), None), Tier::Daily);
    assert_eq!(
        magic_logs::per_top_tier(&spec, &window(7).with("unit_id", ["x"])),
        Tier::DailyTop
    );
}
