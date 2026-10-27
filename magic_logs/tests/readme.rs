//! The README's code, compiled: every Rust block of `README.md` is here,
//! line for line (indentation aside), inside a function that gives it what
//! it names — and `every_rust_block_of_the_readme_is_here` holds the two to
//! each other, so the README cannot show code that does not build.

use chrono::{DateTime, Duration, Utc};
use magic_logs::{Filter, MagicLog};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(sqlx::Type, Debug, Clone, Copy)]
#[sqlx(type_name = "severity")]
pub enum Severity {
    Info,
    Error,
}

async fn hero(
    mut tx: Transaction<'static, Postgres>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<String, Error> {
    use magic_logs::{histogram, migration, Bucket, Filter, MagicLog};

    // A row says what each of its columns is to an explorer.
    #[derive(MagicLog, sqlx::FromRow)]
    #[log(name = "events", schema = "fleet", view_schema = "api")]
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
    }

    // Once, in a migration: the table becomes a hypertable, compressed as it
    // ages and counted by hour and by day, every relation read through a view
    // your authorization writes.
    let sql = migration(&Event::SPEC, govern);

    // In a handler: a week of one kind, by day — read from the daily counts.
    let filter = Filter::new(from, to).with("kind", ["door_opened"]);
    let bars: Vec<Bucket> = histogram(&Event::SPEC, &filter, Duration::days(1))?
        .build_query_as()
        .fetch_all(&mut *tx)
        .await?;

    // The row is read as sqlx reads it, every column.
    let row = |e: Event| (e.id, e.at, e.zone, e.level, e.kind, e.unit, e.message);
    let _ = row;
    Ok(format!("{sql}{}", bars.len()))
}

mod defining {
    use super::*;

    #[derive(MagicLog, sqlx::FromRow)]
    #[log(
        name = "events",
        schema = "telemetry",
        view_schema = "api",
        chunk = "1 day",
        compress_after = "2 days",
        segment_by = "unit_id",
        search_with = "f_unaccent"
    )]
    pub struct Reading {
        #[log(id)]
        pub id: Uuid,
        #[log(time)]
        pub at: DateTime<Utc>,
        #[log(tenant)]
        pub tenant_id: Uuid,
        #[log(scope)]
        pub district_id: Uuid,
        #[log(level, sql = "severity")]
        pub severity: Severity,
        #[log(facet, per_top, indexed)]
        pub kind: String,
        #[log(top)]
        pub unit_id: Uuid,
        #[log(filter)]
        pub operator_id: Option<Uuid>,
        #[log(search)]
        pub description: String,
    }

    /// Every column, as a page reads it.
    pub fn columns(r: Reading) -> impl std::fmt::Debug {
        (
            r.id,
            r.at,
            r.tenant_id,
            r.district_id,
            r.severity,
            r.kind,
            r.unit_id,
            r.operator_id,
            r.description,
        )
    }
}

use magic_logs::Governed;

/// Every relation readable through a view that keeps the reader's zones.
fn govern(g: &Governed) -> String {
    let scope = g.scope.unwrap_or("zone");
    format!(
        "CREATE VIEW {vs}.{v} AS SELECT * FROM {s}.{r} \
          WHERE {scope} = ANY(string_to_array(current_setting('app.zones'), ','));",
        vs = g.view_schema,
        v = g.view,
        s = g.schema,
        r = g.relation,
    )
}

#[derive(MagicLog, sqlx::FromRow)]
#[log(name = "events", schema = "fleet", view_schema = "api")]
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
}

async fn asking(
    mut tx: Transaction<'static, Postgres>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    zones: Vec<String>,
    after: Option<magic_logs::Cursor>,
) -> Result<usize, Error> {
    use magic_logs::{facets, grouped, page, Cursor, FacetCount, Group};

    let filter = Filter::new(from, to)
        .with("level", ["error", "critical"])
        .with("zone", zones)
        .search("seal");

    // The newest rows, a page at a time; the next begins after the last row.
    let rows: Vec<Event> = page(&Event::SPEC, &filter, after.as_ref(), 100)?
        .build_query_as()
        .fetch_all(&mut *tx)
        .await?;
    let after = rows.last().map(|r| Cursor {
        time: r.at,
        id: r.id.to_string(),
    });

    // Each field's values, each counted under every other field's filter.
    let counts: Vec<FacetCount> = facets(&Event::SPEC, &filter, &["level", "kind", "unit"])?
        .build_query_as()
        .fetch_all(&mut *tx)
        .await?;

    // The fifty largest unit × kind groups, with their first and last row.
    let groups: Vec<Group> = grouped(&Event::SPEC, &filter, "kind", 50)?
        .build_query_as()
        .fetch_all(&mut *tx)
        .await?;

    let messages = rows
        .iter()
        .map(|r| (&r.zone, &r.level, &r.kind, &r.unit, &r.message));
    Ok(messages.count() + counts.len() + groups.len() + usize::from(after.is_some()))
}

/// The lines of `text` that hold code, as rustfmt leaves them, without
/// their indentation.
fn code(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Every function above is the README's code, and only compiled: named here,
/// none is dead.
#[test]
fn the_readmes_code_compiles() {
    let _ = (hero, asking);
    let _ = defining::columns;
    let _ = PgPool::connect_lazy;
}

/// The README's governing view, on the SPEC its examples use.
#[test]
fn the_readmes_govern_writes_a_view_per_relation() {
    let sql = magic_logs::migration(&Event::SPEC, govern);
    assert_eq!(sql.matches("CREATE VIEW api.events").count(), 4);
}

#[test]
fn every_rust_block_of_the_readme_is_here() {
    let root = env!("CARGO_MANIFEST_DIR");
    let readme = std::fs::read_to_string(format!("{root}/../README.md")).unwrap();
    let here = code(&std::fs::read_to_string(format!("{root}/tests/readme.rs")).unwrap());
    let mut blocks = 0;
    let mut rest = readme.as_str();
    while let Some(start) = rest.find("```rust\n") {
        let body = &rest[start + 8..];
        let end = body.find("```").expect("a block that ends");
        let block = code(&body[..end]);
        assert!(
            here.windows(block.len()).any(|w| w == block.as_slice()),
            "a Rust block of the README is not in tests/readme.rs as it is:\n{}",
            block.join("\n")
        );
        blocks += 1;
        rest = &body[end..];
    }
    assert!(blocks >= 4, "the README's Rust blocks: {blocks}");
}
