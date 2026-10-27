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

#[derive(MagicLog, sqlx::FromRow, Debug, Clone)]
#[log(
    name = "events",
    schema = "private",
    chunk = "1 day",
    compress_after = "2 days",
    segment_by = "unit_id",
    search_with = "fold"
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
    #[log(filter)]
    seen_at: Option<DateTime<Utc>>,
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
        "CREATE VIEW {} AS SELECT * FROM {};",
        g.quoted_view(),
        g.quoted_relation()
    )
}

async fn setup(pool: &PgPool) {
    setup_with(pool, plain).await
}

/// Each relation readable through a view that keeps one zone, when the
/// session names one: the scope narrowing every tier alike.
fn zoned(g: &Governed) -> String {
    format!(
        "CREATE VIEW {} AS SELECT * FROM {} \
          WHERE (SELECT coalesce(current_setting('test.zone', true), '')) = '' \
             OR {scope} = (SELECT current_setting('test.zone', true))::uuid;",
        g.quoted_view(),
        g.quoted_relation(),
        scope = g.quoted_scope().unwrap(),
    )
}

async fn refresh(pool: &PgPool) {
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

async fn setup_with(pool: &PgPool, govern: fn(&Governed) -> String) {
    sqlx::raw_sql(
        "CREATE EXTENSION IF NOT EXISTS timescaledb; \
         CREATE FUNCTION fold(t text) RETURNS text IMMUTABLE LANGUAGE sql \
             AS $$ SELECT translate(lower(t), '％＿＼', '%_\\') $$; \
         CREATE TYPE level AS ENUM ('Info', 'Warning', 'Error'); \
         CREATE SCHEMA private; \
         CREATE TABLE private.events (id uuid NOT NULL, at timestamptz NOT NULL, tenant_id uuid NOT NULL, \
             zone_id uuid NOT NULL, level level NOT NULL, kind text NOT NULL, unit_id uuid NOT NULL, \
             operator text, description text NOT NULL, seen_at timestamptz, PRIMARY KEY (id, at));",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::raw_sql(&migration(&Event::SPEC, govern))
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
                'unit ' || u || ' said ' || CASE WHEN n % 10 = 0 THEN 'hola mundo' ELSE 'ok' END, \
                CASE WHEN n % 4 = 0 THEN '{start}'::timestamptz END \
           FROM generate_series(0, 4 * 144 - 1) n, generate_series(1, 3) u;",
        start = day().to_rfc3339()
    ))
    .execute(pool)
    .await
    .unwrap();
    refresh(pool).await;
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
        let raw: Vec<Bucket> = histogram(&spec, &filter.clone().search("unit"), step)
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
    assert_eq!(magic_logs::counts_tier(&spec, &filter, None), Tier::Daily);
    assert_eq!(magic_logs::per_top_tier(&spec, &filter), Tier::DailyTop);
    let mut counted: Vec<FacetCount> =
        facets(&spec, &filter, &["kind", "level", "zone_id", "unit_id"])
            .unwrap()
            .build_query_as()
            .fetch_all(&pool)
            .await
            .unwrap();
    let mut raw: Vec<FacetCount> = facets(
        &spec,
        &filter.clone().search("unit"),
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
    let raw: Vec<Group> = grouped(&spec, &window(4).search("unit"), "kind", 5)
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
    // A page is the whole row, every column as the table holds it.
    let zones = [Uuid::from_u128(1), Uuid::from_u128(2)];
    assert!(first.iter().all(|e| e.tenant_id == TENANT
        && zones.contains(&e.zone_id)
        && e.unit_id
            .to_string()
            .starts_with("00000000-0000-0000-0000-0000000000a")
        && matches!(e.level, Level::Info | Level::Warning | Level::Error)
        && e.description.starts_with("unit ")
        && e.seen_at.is_none_or(|t| t == day())));

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
    // The tenant is kept in every count, and filtered like a facet.
    let tenant = window(1).with("tenant_id", [TENANT.to_string()]);
    assert_eq!(tenant.check(&spec), Ok(()));
    assert_eq!(tier(&tenant, Some(Duration::hours(1))), Tier::Hourly);
    for step in [
        Duration::zero(),
        Duration::nanoseconds(1),
        Duration::hours(1) + Duration::nanoseconds(1),
    ] {
        assert_eq!(
            histogram(&spec, &window(1), step).err(),
            Some(magic_logs::Error::Step),
            "{step}"
        );
    }
    assert_eq!(
        tier(
            &window(1),
            Some(Duration::hours(1) + Duration::milliseconds(1))
        ),
        Tier::Raw
    );
    assert_eq!(tier(&window(7), Some(Duration::hours(6))), Tier::Hourly);
    assert_eq!(tier(&window(7), Some(Duration::days(1))), Tier::Daily);
    assert_eq!(tier(&window(2), None), Tier::Hourly);
    assert_eq!(tier(&window(7), None), Tier::Daily);
    assert_eq!(
        magic_logs::per_top_tier(&spec, &window(7).with("unit_id", ["x"])),
        Tier::DailyTop
    );
}

/// The JSON an explorer reads, from a database: what
/// `explorer/src/core/contract/fixtures/answers.ts` holds. The explorer's
/// type-check reads it against the generated types and its tests read it;
/// a change on either side shows on the other. `UPDATE_CONTRACT=1` writes it.
#[cfg(feature = "serde")]
#[sqlx::test(migrations = false)]
async fn the_json_an_explorer_reads_is_the_contract(pool: PgPool) {
    setup(&pool).await;
    let spec = Event::SPEC;
    let filter = window(1).with("kind", ["door", "power"]);
    let buckets: Vec<Bucket> = histogram(&spec, &filter, Duration::hours(6))
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    let mut counts: Vec<FacetCount> = facets(&spec, &filter, &["level", "kind", "unit_id"])
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    counts.sort_by(|a, b| (&a.field, &a.value).cmp(&(&b.field, &b.value)));
    let groups: Vec<Group> = grouped(&spec, &filter, "kind", 4)
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "question": {
            "from": filter.from,
            "to": filter.to,
            "fields": { "kind": ["door", "power"] },
        },
        "step": "sixHours",
        "histogram": buckets,
        "facets": counts,
        "grouped": groups,
    }))
    .unwrap();
    let text = format!(
        "// Written by magic_logs' `the_json_an_explorer_reads_is_the_contract`: \
         UPDATE_CONTRACT=1 rewrites it.\n\
         import type {{ BucketRow, FacetCountRow, GroupRow, Question }} from '../types.js'\n\n\
         export default {json} satisfies {{\n  question: Question\n  step: string\n  \
         histogram: BucketRow[]\n  facets: FacetCountRow[]\n  grouped: GroupRow[]\n}}\n"
    );
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../explorer/src/core/contract/fixtures/answers.ts"
    );
    if std::env::var_os("UPDATE_CONTRACT").is_some() {
        std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
        std::fs::write(path, &text).unwrap();
    }
    let written = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        written == text,
        "explorer/src/core/contract/fixtures/answers.ts is not what the crate answers: \
         UPDATE_CONTRACT=1 rewrites it"
    );
}

/// The histogram of `filter` by `step`, from the tier it picks and from the
/// rows (a search of what every test row says reads them), sorted alike.
async fn both_histograms(
    pool: &PgPool,
    filter: &Filter,
    step: Duration,
) -> (Vec<Bucket>, Vec<Bucket>) {
    let spec = Event::SPEC;
    let counted: Vec<Bucket> = histogram(&spec, filter, step)
        .unwrap()
        .build_query_as()
        .fetch_all(pool)
        .await
        .unwrap();
    let raw: Vec<Bucket> = histogram(&spec, &filter.clone().search("unit"), step)
        .unwrap()
        .build_query_as()
        .fetch_all(pool)
        .await
        .unwrap();
    (counted, raw)
}

async fn both_facets(
    pool: &PgPool,
    filter: &Filter,
    fields: &[&str],
) -> (Vec<FacetCount>, Vec<FacetCount>) {
    let spec = Event::SPEC;
    let mut answers = Vec::new();
    for f in [filter.clone(), filter.clone().search("unit")] {
        let mut rows: Vec<FacetCount> = facets(&spec, &f, fields)
            .unwrap()
            .build_query_as()
            .fetch_all(pool)
            .await
            .unwrap();
        rows.sort_by(|a, b| (&a.field, &a.value).cmp(&(&b.field, &b.value)));
        answers.push(rows);
    }
    let raw = answers.pop().unwrap();
    (answers.pop().unwrap(), raw)
}

#[sqlx::test(migrations = false)]
async fn a_question_narrowed_to_one_top_reads_its_daily_counts(pool: PgPool) {
    setup(&pool).await;
    let spec = Event::SPEC;
    let unit = "00000000-0000-0000-0000-0000000000a2";
    let filter = window(4).with("unit_id", [unit]).with("kind", ["door"]);
    assert_eq!(
        magic_logs::counts_tier(&spec, &filter, Some(Duration::days(1))),
        Tier::DailyTop
    );
    let (counted, raw) = both_histograms(&pool, &filter, Duration::days(1)).await;
    assert_eq!(counted, raw);
    assert!(!counted.is_empty());
    let (counted, raw) = both_facets(&pool, &filter, &["level", "kind", "unit_id"]).await;
    assert_eq!(counted, raw);
    // By the hour, the counts per day cannot answer: the rows do.
    assert_eq!(
        magic_logs::counts_tier(&spec, &filter, Some(Duration::hours(1))),
        Tier::Raw
    );
}

#[sqlx::test(migrations = false)]
async fn a_histogram_counts_each_bucket_the_window_touches_whole(pool: PgPool) {
    setup(&pool).await;
    // A window that begins and ends inside a bucket, at every tier: the
    // counts and the rows alike count those buckets whole.
    let unaligned = Filter::new(
        day() + Duration::minutes(20),
        day() + Duration::days(2) + Duration::hours(5) + Duration::minutes(10),
    );
    // From the start of the bucket the window begins in to the end of the
    // one it ends in (every step here divides a day, so buckets count from
    // midnight UTC).
    let whole = |step: Duration| {
        let s = step.num_seconds();
        let floor = |t: DateTime<Utc>| t.timestamp() - t.timestamp().rem_euclid(s);
        let at = |secs: i64| Utc.timestamp_opt(secs, 0).unwrap();
        Filter::new(
            at(floor(unaligned.from)),
            at(floor(unaligned.to - Duration::seconds(1)) + s),
        )
    };
    for step in [
        Duration::hours(1),
        Duration::hours(6),
        Duration::days(1),
        Duration::minutes(10),
    ] {
        let (counted, raw) = both_histograms(&pool, &unaligned, step).await;
        assert_eq!(counted, raw, "by {step}");
        let (exact, _) = both_histograms(&pool, &whole(step), step).await;
        assert_eq!(counted, exact, "by {step}: the buckets touched, whole");
    }
}

#[sqlx::test(migrations = false)]
async fn new_rows_count_at_once_and_late_ones_after_the_refresh(pool: PgPool) {
    setup(&pool).await;
    let insert = |at: DateTime<Utc>| {
        format!(
            "INSERT INTO private.events SELECT gen_random_uuid(), '{}'::timestamptz + n * interval '1 second', \
             '{TENANT}', '00000000-0000-0000-0000-000000000001', 'Error', 'door', \
             '00000000-0000-0000-0000-0000000000a1', NULL, 'unit 1 came late' FROM generate_series(1, 25) n",
            at.to_rfc3339()
        )
    };
    let filter = window(6);
    // After everything refreshed: read from what is not materialized yet.
    sqlx::raw_sql(&insert(day() + Duration::days(5)))
        .execute(&pool)
        .await
        .unwrap();
    let (counted, raw) = both_histograms(&pool, &filter, Duration::days(1)).await;
    assert_eq!(counted, raw, "a new row counts before any refresh");

    // Into a day already counted: missing until the refresh recounts it.
    sqlx::raw_sql(&insert(day() + Duration::days(1) + Duration::hours(3)))
        .execute(&pool)
        .await
        .unwrap();
    let (counted, raw) = both_histograms(&pool, &filter, Duration::days(1)).await;
    assert_ne!(counted, raw);
    refresh(&pool).await;
    let (counted, raw) = both_histograms(&pool, &filter, Duration::days(1)).await;
    assert_eq!(counted, raw, "the refresh recounts the late day");
}

#[sqlx::test(migrations = false)]
async fn compressed_history_pages_and_counts_as_it_did(pool: PgPool) {
    setup(&pool).await;
    let spec = Event::SPEC;
    let filter = window(4).with("kind", ["power"]);
    let pages = |pool: PgPool| {
        let filter = filter.clone();
        async move {
            let mut out = Vec::new();
            let mut after: Option<Cursor> = None;
            loop {
                let rows: Vec<Event> = page(&spec, &filter, after.as_ref(), 40)
                    .unwrap()
                    .build_query_as()
                    .fetch_all(&pool)
                    .await
                    .unwrap();
                let Some(last) = rows.last() else { break };
                after = Some(Cursor {
                    time: last.at,
                    id: last.id.to_string(),
                });
                out.extend(rows.iter().map(|e| (e.at, e.id)));
            }
            out
        }
    };
    let before = pages(pool.clone()).await;
    let (counted_before, _) = both_histograms(&pool, &filter, Duration::days(1)).await;
    let compressed: i64 =
        sqlx::query_scalar("SELECT count(compress_chunk(c)) FROM show_chunks('private.events') c")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(compressed >= 4);
    assert_eq!(pages(pool.clone()).await, before);
    let powered: i64 =
        sqlx::query_scalar("SELECT count(*) FROM private.events WHERE kind = 'power'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before.len() as i64, powered);
    let (counted, raw) = both_histograms(&pool, &filter, Duration::days(1)).await;
    assert_eq!(counted, counted_before);
    assert_eq!(counted, raw);

    // A filter on a timestamp, over compressed chunks: the values' cast is
    // a constant to TimescaleDB's vectorized filter.
    let seen = window(4).with("seen_at", [day().to_rfc3339()]);
    let rows: Vec<Event> = page(&spec, &seen, None, 10_000)
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 144 * 3);
    let bad = page(
        &spec,
        &window(4).with("seen_at", ["yesterday-ish"]),
        None,
        10,
    )
    .unwrap()
    .build_query_as::<Event>()
    .fetch_all(&pool)
    .await
    .unwrap_err();
    let code = bad
        .as_database_error()
        .and_then(|e| e.code())
        .unwrap_or_default();
    assert!(code.starts_with("22"), "{code}: {bad}");
}

#[sqlx::test(migrations = false)]
async fn a_scoped_reader_counts_only_its_scope_in_every_tier(pool: PgPool) {
    setup_with(&pool, zoned).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('test.zone', '00000000-0000-0000-0000-000000000001', true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let spec = Event::SPEC;
    for (filter, step) in [
        (window(1), Duration::hours(1)),
        (window(4), Duration::days(1)),
        (
            window(4).with("unit_id", ["00000000-0000-0000-0000-0000000000a1"]),
            Duration::days(1),
        ),
    ] {
        let counted: Vec<Bucket> = histogram(&spec, &filter, step)
            .unwrap()
            .build_query_as()
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        let raw: Vec<Bucket> = histogram(&spec, &filter.clone().search("unit"), step)
            .unwrap()
            .build_query_as()
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        assert_eq!(counted, raw, "{filter:?}");
    }
    let zones: Vec<FacetCount> = facets(&spec, &window(4), &["zone_id"])
        .unwrap()
        .build_query_as()
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(zones.len(), 1, "one zone is all this reader sees");
}

#[sqlx::test(migrations = false)]
async fn a_search_reads_its_text_as_typed(pool: PgPool) {
    setup(&pool).await;
    sqlx::raw_sql(&format!(
        "INSERT INTO private.events SELECT gen_random_uuid(), '{at}'::timestamptz + n * interval '1 minute', \
         '{TENANT}', '00000000-0000-0000-0000-000000000001', 'Info', 'cpu', \
         '00000000-0000-0000-0000-0000000000a1', NULL, \
         CASE WHEN n % 2 = 0 THEN 'cpu at 50% now' ELSE 'cpu at 507 now\\' END \
         FROM generate_series(1, 20) n",
        at = (day() + Duration::hours(2)).to_rfc3339()
    ))
    .execute(&pool)
    .await
    .unwrap();
    let found = |text: &'static str| {
        let pool = pool.clone();
        async move {
            let rows: Vec<Event> = page(&Event::SPEC, &window(1).search(text), None, 1000)
                .unwrap()
                .build_query_as()
                .fetch_all(&pool)
                .await
                .unwrap();
            rows.len()
        }
    };
    assert_eq!(found("50%").await, 10);
    // The function folds `％` into `%` before the text is escaped.
    assert_eq!(found("50％").await, 10);
    assert_eq!(found("cpu＿at").await, 0);
    // A blank search is no search: every row, NULL columns and all.
    let all: Vec<Event> = page(&Event::SPEC, &window(1).search("  "), None, 10_000)
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(all.len(), 144 * 3 + 20);
    assert_eq!(found("now\\").await, 10);
    assert_eq!(found("cpu_at").await, 0);
}

#[sqlx::test(migrations = false)]
async fn a_table_of_only_a_time_and_an_id_is_made_a_log_table(pool: PgPool) {
    #[derive(MagicLog)]
    #[log(name = "pings", schema = "private", view_schema = "public")]
    struct Ping {
        #[log(id)]
        id: i64,
        #[log(time)]
        at: DateTime<Utc>,
    }
    let _ = |p: Ping| (p.id, p.at);
    sqlx::raw_sql(
        "CREATE EXTENSION IF NOT EXISTS timescaledb; CREATE SCHEMA private; \
         CREATE TABLE private.pings (id bigint NOT NULL, at timestamptz NOT NULL, PRIMARY KEY (id, at)); \
         INSERT INTO private.pings SELECT n, '2026-10-01'::timestamptz + n * interval '1 minute' \
           FROM generate_series(1, 300) n;",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(&migration(&Ping::SPEC, plain))
        .execute(&pool)
        .await
        .unwrap();
    let bars: Vec<Bucket> = histogram(&Ping::SPEC, &window(1), Duration::hours(1))
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(bars.iter().map(|b| b.count).sum::<i64>(), 300);
}

#[sqlx::test(migrations = false)]
async fn a_short_window_counts_its_tops_from_the_rows(pool: PgPool) {
    setup(&pool).await;
    let spec = Event::SPEC;
    let begins = day() + Duration::hours(10);
    let filter = Filter::new(begins, begins + Duration::minutes(30));
    assert_eq!(magic_logs::per_top_tier(&spec, &filter), Tier::Raw);
    let (counted, raw) = both_facets(&pool, &filter, &["unit_id"]).await;
    assert_eq!(counted, raw);
    assert_eq!(counted.iter().map(|f| f.count).sum::<i64>(), 3 * 3);
    let groups: Vec<Group> = grouped(&spec, &filter, "kind", 50)
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(groups
        .iter()
        .all(|g| g.first_at.unwrap() >= filter.from && g.last_at.unwrap() < filter.to));
}

#[sqlx::test(migrations = false)]
async fn names_sql_must_quote_and_a_tenant_that_is_the_top_migrate(pool: PgPool) {
    #[derive(MagicLog)]
    #[log(name = "Events", schema = "Private", view_schema = "Api")]
    struct Reading {
        #[log(id)]
        id: i64,
        #[log(time)]
        at: DateTime<Utc>,
        #[log(tenant, top)]
        unit_id: i64,
        #[log(facet, per_top)]
        kind: String,
        // Named like the counts' own column, and not counted: the counts
        // group by their bucket, not by it.
        #[log(search)]
        bucket: String,
    }
    let _ = |r: Reading| (r.id, r.at, r.unit_id, r.kind, r.bucket);
    sqlx::raw_sql(
        "CREATE EXTENSION IF NOT EXISTS timescaledb; CREATE SCHEMA \"Private\"; CREATE SCHEMA \"Api\"; \
         CREATE TABLE \"Private\".\"Events\" (id bigint NOT NULL, at timestamptz NOT NULL, \
             unit_id bigint NOT NULL, kind text NOT NULL, bucket text NOT NULL, PRIMARY KEY (id, at)); \
         INSERT INTO \"Private\".\"Events\" \
           SELECT n, '2026-10-01'::timestamptz + n * interval '10 minutes', n % 3, 'k' || n % 2, 's3://logs' \
           FROM generate_series(1, 600) n;",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(&migration(&Reading::SPEC, plain))
        .execute(&pool)
        .await
        .unwrap();
    let filter = Filter::new(day(), day() + Duration::days(5));
    let counts: Vec<FacetCount> = facets(&Reading::SPEC, &filter, &["unit_id", "kind"])
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    let total = |field: &str| -> i64 {
        counts
            .iter()
            .filter(|c| c.field == field)
            .map(|c| c.count)
            .sum()
    };
    assert_eq!((total("unit_id"), total("kind")), (600, 600));
    let groups: Vec<Group> = grouped(&Reading::SPEC, &filter, "kind", 10)
        .unwrap()
        .build_query_as()
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(groups.iter().map(|g| g.count).sum::<i64>(), 600);
}
