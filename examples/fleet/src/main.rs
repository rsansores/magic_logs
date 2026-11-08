//! The fleet: thirty days of a delivery fleet's events in one TimescaleDB
//! table, a few more every second, and the explorer's four questions served
//! as JSON — as `magic_logs_explorer` reads them. Beside them, the trucks'
//! positions: a table stored and paged, never counted.
//!
//! A range is a field's `min..max` in the query string, either side empty
//! (`odometer=10000..`), as `magic_logs_explorer`'s `between` writes it.
//!
//! A reader sees the zones the `x-zones` header names (`*`: every zone; no
//! header: none), and the views every question reads through keep only
//! their rows. That is the host's authorization, which magic_logs leaves to
//! the host: a real one takes the reader's zones from its session, not from
//! a header the reader writes.

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use magic_logs::{
    drift, facets, grouped, histogram, migration, page, Bucket, Condition, Cursor, FacetCount,
    Filter, Governed, Group, MagicLog, Spec,
};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(MagicLog, sqlx::FromRow, serde::Serialize)]
#[log(
    name = "events",
    schema = "fleet",
    view_schema = "api",
    segment_by = "unit"
)]
pub struct Event {
    #[log(id)]
    pub id: Uuid,
    #[log(time)]
    pub at: DateTime<Utc>,
    #[log(scope)]
    pub zone: String,
    #[log(level)]
    pub level: String,
    #[log(facet, per_top, indexed)]
    pub kind: String,
    #[log(top)]
    pub unit: String,
    #[log(search)]
    pub message: String,
    /// The truck's odometer when it happened, in km: filtered by range.
    #[log(filter, minmax)]
    pub odometer: Option<i64>,
}

/// Where a truck was: counted by nothing, so stored as its rows alone,
/// compressed per truck, and read by the page.
#[derive(MagicLog, sqlx::FromRow, serde::Serialize)]
#[log(
    name = "positions",
    schema = "fleet",
    view_schema = "api",
    segment_by = "unit"
)]
pub struct Position {
    #[log(id)]
    pub id: Uuid,
    #[log(time)]
    pub at: DateTime<Utc>,
    #[log(scope)]
    pub zone: String,
    #[log(filter)]
    pub unit: String,
    pub lat: f64,
    pub lon: f64,
    /// km/h: a range finds the trucks that sped.
    #[log(filter, minmax)]
    pub speed: i32,
}

/// The most bars a histogram is drawn in.
const MOST_BARS: i64 = 1_000;

/// The fields a reader may filter and count by.
const FIELDS: [&str; 4] = ["level", "kind", "zone", "unit"];

/// The fields a reader may hold between two bounds, each its table's.
const RANGES: [&str; 2] = ["odometer", "speed"];

/// Each relation read through a view that keeps the zones the reader may
/// see: every zone for `*`, none when `fleet.zones` is not set. The setting
/// is read in a scalar subquery, once per query rather than once per row.
fn govern(g: &Governed) -> String {
    let scope = g
        .quoted_scope()
        .expect("the fleet's events are scoped by zone");
    format!(
        "CREATE VIEW {} AS SELECT * FROM {} \
          WHERE (SELECT current_setting('fleet.zones', true)) = '*' \
             OR {scope} = ANY((SELECT string_to_array(current_setting('fleet.zones', true), ','))::text[]);",
        g.quoted_view(),
        g.quoted_relation(),
    )
}

type Error = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://fleet:fleet@localhost:5435/fleet".into());
    let pool = PgPool::connect(&url).await?;
    set_up(&pool).await?;
    tokio::spawn(drive(pool.clone()));

    let app = Router::new()
        .route("/api/page", get(page_of))
        .route("/api/histogram", get(histogram_of))
        .route("/api/facets", get(facets_of))
        .route("/api/grouped", get(grouped_of))
        .route("/api/positions", get(positions_of))
        .with_state(pool);
    let listen = std::env::var("FLEET_LISTEN").unwrap_or_else(|_| "127.0.0.1:3001".into());
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    println!("the fleet answers at http://{listen}/api/…");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

/// The table, made a log table once; thirty days of events when it is empty.
async fn set_up(pool: &PgPool) -> Result<(), Error> {
    sqlx::raw_sql(include_str!("../schema.sql"))
        .execute(pool)
        .await?;
    // Each table made a log table once: a database a build before the
    // positions made has events, and no positions yet.
    for spec in [Event::SPEC, Position::SPEC] {
        let made: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM timescaledb_information.hypertables \
              WHERE hypertable_schema = $1 AND hypertable_name = $2)",
        )
        .bind(spec.schema)
        .bind(spec.name)
        .fetch_one(pool)
        .await?;
        if !made {
            let mut tx = pool.begin().await?;
            sqlx::raw_sql(&migration(&spec, govern))
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
    }
    // A table made by an older build is the one the code declares, or says why not.
    let mut conn = pool.acquire().await?;
    for spec in [Event::SPEC, Position::SPEC] {
        for d in drift(&mut conn, &spec).await? {
            eprintln!("drift: {d}");
        }
    }
    let empty: bool = sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM fleet.events)")
        .fetch_one(pool)
        .await?;
    if empty {
        println!("seeding thirty days of events…");
        sqlx::raw_sql(SEED).execute(pool).await?;
        // The counts are the policies' to refresh, from now: until they
        // have, what is not counted yet is read from the rows.
    }
    Ok(())
}

/// Every two minutes of thirty days, busier by day: 36 trucks in 5 zones,
/// and an incident in the harbor an hour and a half ago. A truck's zone is
/// its number's: truck-04, -09, … -34 work the harbor.
const SEED: &str = "
WITH kinds(kind, level, says, weight) AS (VALUES
    ('engine_start', 'info', ' started its engine', 30),
    ('door_opened', 'info', ': cargo door opened', 24),
    ('geofence_exit', 'warning', ' left its assigned area', 9),
    ('low_battery', 'warning', ': battery at 18 %', 8),
    ('gps_lost', 'warning', ' lost its GPS fix', 6),
    ('temperature_high', 'error', ': cold chain above 8 °C', 3),
    ('sync_failed', 'error', ' could not upload its log', 2),
    ('tamper', 'critical', ': seal broken', 1)),
picks AS (
    SELECT t, n, random() * 83 AS roll, 1 + floor(random() * 36)::int AS truck
      FROM generate_series(now() - interval '30 days', now(), interval '2 minutes') t,
           generate_series(1, 6) n
     WHERE random() < 0.25 + 0.75 * greatest(0, sin((extract(hour FROM t) - 5) / 24 * 2 * pi()))),
weighted AS (
    SELECT kind, level, says, sum(weight) OVER (ORDER BY weight DESC, kind) AS upto FROM kinds)
INSERT INTO fleet.events
SELECT gen_random_uuid(), p.t + random() * interval '2 minutes', w.level, w.kind,
       (ARRAY['North', 'South', 'East', 'West', 'Harbor'])[1 + p.truck % 5],
       'truck-' || lpad(p.truck::text, 2, '0'),
       'truck-' || lpad(p.truck::text, 2, '0') || w.says,
       p.truck * 10000 + extract(epoch FROM p.t - (now() - interval '30 days'))::bigint / 600
  FROM picks p
  CROSS JOIN LATERAL (SELECT * FROM weighted WHERE upto > p.roll ORDER BY upto LIMIT 1) w;

INSERT INTO fleet.events
SELECT gen_random_uuid(), now() - interval '90 minutes' + random() * random() * interval '40 minutes',
       k.level, k.kind, 'Harbor', r.unit, r.unit || k.says, NULL
  FROM (SELECT g, 'truck-' || lpad((4 + 5 * floor(random() * 7))::text, 2, '0') AS unit, random() AS roll
          FROM generate_series(1, 260) g) r,
       LATERAL (SELECT * FROM (VALUES (0.45, 'temperature_high', 'error', ': cold chain above 8 °C'),
                                      (0.85, 'sync_failed', 'error', ' could not upload its log'),
                                      (1.0, 'tamper', 'critical', ': seal broken')) v(upto, kind, level, says)
                 WHERE v.upto > r.roll ORDER BY v.upto LIMIT 1) k;

INSERT INTO fleet.positions
SELECT gen_random_uuid(), t, 'truck-' || lpad(u::text, 2, '0'),
       (ARRAY['North', 'South', 'East', 'West', 'Harbor'])[1 + u % 5],
       19.4 + u / 100.0 + random() / 50, -99.1 + u / 100.0 + random() / 50,
       floor(random() * random() * 120)::int
  FROM generate_series(now() - interval '7 days', now(), interval '1 minute') t, generate_series(1, 36) u;
";

/// A few events a second, for a live explorer.
async fn drive(pool: PgPool) {
    let mut every = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        every.tick().await;
        let _ = sqlx::query(
            "INSERT INTO fleet.events
             SELECT gen_random_uuid(), now(), 'info', 'engine_start',
                    (ARRAY['North', 'South', 'East', 'West', 'Harbor'])[1 + t % 5],
                    'truck-' || lpad(t::text, 2, '0'),
                    'truck-' || lpad(t::text, 2, '0') || ' started its engine'
               FROM (SELECT 1 + floor(random() * 36)::int AS t
                       FROM generate_series(1, floor(random() * 3)::int)) r",
        )
        .execute(&pool)
        .await;
    }
}

/// What a request asks: the window, each field's values, the search.
/// A request's query string: a key may come more than once (`kind=a&kind=b`),
/// so a value may hold a comma.
type Params = Vec<(String, String)>;

fn one<'a>(q: &'a Params, key: &str) -> Option<&'a str> {
    q.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn all<'a>(q: &'a Params, key: &'a str) -> impl Iterator<Item = &'a str> {
    q.iter()
        .filter(move |(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn filter_of(q: &Params, spec: &Spec) -> Result<Filter, Refused> {
    let instant = |key: &str| {
        one(q, key)
            .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
            .map(|t| t.with_timezone(&Utc))
            .ok_or_else(|| Refused::Question(format!("`{key}` is an RFC 3339 instant")))
    };
    let mut filter = Filter::new(instant("from")?, instant("to")?);
    for field in FIELDS.into_iter().filter(|f| spec.field(f).is_some()) {
        let values: Vec<&str> = all(q, field).collect();
        if !values.is_empty() {
            filter = filter.with(field, values);
        }
    }
    for field in RANGES.into_iter().filter(|f| spec.field(f).is_some()) {
        if let Some(raw) = one(q, field) {
            filter = filter.with_condition(field, between(field, raw)?);
        }
    }
    if let Some(text) = one(q, "search").filter(|v| !v.trim().is_empty()) {
        filter = filter.search(text.trim());
    }
    filter.check(spec)?;
    Ok(filter)
}

/// `min..max`, either side empty, as the explorer's `between` writes it: a
/// backslash keeps the character after it, so a bound may hold `..`.
fn between(field: &str, raw: &str) -> Result<Condition, Refused> {
    let (mut min, mut max, mut past) = (String::new(), String::new(), false);
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        let c = match c {
            '\\' => chars.next().unwrap_or(c),
            '.' if !past && chars.peek() == Some(&'.') => {
                chars.next();
                past = true;
                continue;
            }
            _ => c,
        };
        if past {
            max.push(c)
        } else {
            min.push(c)
        }
    }
    if !past {
        return Err(Refused::Question(format!(
            "`{field}` is a range: `min..max`"
        )));
    }
    let bound = |b: String| (!b.is_empty()).then_some(b);
    Ok(Condition::Between {
        min: bound(min),
        max: bound(max),
    })
}

/// The reader's transaction: narrowed to the zones `x-zones` names.
async fn reader(
    pool: &PgPool,
    headers: &HeaderMap,
) -> Result<Transaction<'static, Postgres>, Refused> {
    let mut tx = pool.begin().await?;
    let zones = headers
        .get("x-zones")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    sqlx::query("SELECT set_config('fleet.zones', $1, true)")
        .bind(zones)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

#[derive(serde::Serialize)]
struct Page {
    rows: Vec<Event>,
    /// Where the next page begins: the last row's time and id.
    next: Option<String>,
}

async fn page_of(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(q): Query<Params>,
) -> Result<Json<Page>, Refused> {
    let filter = filter_of(&q, &Event::SPEC)?;
    let cursor = match one(&q, "cursor") {
        Some(c) => {
            let (time, id) = c
                .split_once('|')
                .ok_or_else(|| Refused::Question("`cursor` is a page's `next`".into()))?;
            let time = DateTime::parse_from_rfc3339(time)
                .map_err(|_| Refused::Question("`cursor` is a page's `next`".into()))?
                .with_timezone(&Utc);
            Some(Cursor {
                time,
                id: id.to_owned(),
            })
        }
        None => None,
    };
    let limit = 50;
    let mut tx = reader(&pool, &headers).await?;
    let rows: Vec<Event> = page(&Event::SPEC, &filter, cursor.as_ref(), limit)?
        .build_query_as()
        .fetch_all(&mut *tx)
        .await?;
    let next = (rows.len() as i64 == limit)
        .then(|| {
            rows.last()
                .map(|r| format!("{}|{}", r.at.to_rfc3339(), r.id))
        })
        .flatten();
    Ok(Json(Page { rows, next }))
}

async fn histogram_of(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(q): Query<Params>,
) -> Result<Json<Vec<Bucket>>, Refused> {
    let filter = filter_of(&q, &Event::SPEC)?;
    let step = match one(&q, "step") {
        Some("minute") => Duration::minutes(1),
        Some("fiveMinutes") => Duration::minutes(5),
        Some("hour") => Duration::hours(1),
        Some("sixHours") => Duration::hours(6),
        Some("day") => Duration::days(1),
        _ => {
            return Err(Refused::Question(
                "`step` is one of the explorer's steps".into(),
            ))
        }
    };
    // A finer step reads the rows: a month by the minute is refused.
    if (filter.to - filter.from).num_seconds() / step.num_seconds() > MOST_BARS {
        return Err(Refused::Question(format!(
            "at most {MOST_BARS} bars: a coarser `step`, or a shorter window"
        )));
    }
    let mut tx = reader(&pool, &headers).await?;
    Ok(Json(
        histogram(&Event::SPEC, &filter, step)?
            .build_query_as()
            .fetch_all(&mut *tx)
            .await?,
    ))
}

async fn facets_of(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(q): Query<Params>,
) -> Result<Json<Vec<FacetCount>>, Refused> {
    let filter = filter_of(&q, &Event::SPEC)?;
    let mut tx = reader(&pool, &headers).await?;
    Ok(Json(
        facets(&Event::SPEC, &filter, &FIELDS)?
            .build_query_as()
            .fetch_all(&mut *tx)
            .await?,
    ))
}

async fn grouped_of(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(q): Query<Params>,
) -> Result<Json<Vec<Group>>, Refused> {
    let filter = filter_of(&q, &Event::SPEC)?;
    let mut tx = reader(&pool, &headers).await?;
    Ok(Json(
        grouped(&Event::SPEC, &filter, "kind", 50)?
            .build_query_as()
            .fetch_all(&mut *tx)
            .await?,
    ))
}

/// The newest positions of the trucks the filter names, one page of them.
async fn positions_of(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(q): Query<Params>,
) -> Result<Json<Vec<Position>>, Refused> {
    let filter = filter_of(&q, &Position::SPEC)?;
    let mut tx = reader(&pool, &headers).await?;
    Ok(Json(
        page(&Position::SPEC, &filter, None, 100)?
            .build_query_as()
            .fetch_all(&mut *tx)
            .await?,
    ))
}

/// A question refused (400), or a database that failed (500).
enum Refused {
    Question(String),
    Database(sqlx::Error),
}

impl From<magic_logs::Error> for Refused {
    fn from(e: magic_logs::Error) -> Self {
        Self::Question(e.to_string())
    }
}

impl From<sqlx::Error> for Refused {
    /// A value the database could not read as its column's type — a cursor
    /// or a filter value that is not one — is the caller's mistake.
    fn from(e: sqlx::Error) -> Self {
        match e.as_database_error().and_then(|d| d.code()) {
            // A data exception: a value that does not read as its column's.
            Some(code) if code.starts_with("22") => Self::Question(e.to_string()),
            _ => Self::Database(e),
        }
    }
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        match self {
            Self::Question(why) => (StatusCode::BAD_REQUEST, why).into_response(),
            Self::Database(e) => {
                eprintln!("database: {e}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}
