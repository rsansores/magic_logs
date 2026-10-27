//! The SQL the crate writes, checked in: the migration, and every question
//! at each table it can be read from. A change to either is a change a host
//! runs, read in review as SQL. `MAGIC_LOGS_BLESS=1` writes them anew.

use chrono::{DateTime, Duration, TimeZone, Utc};
use magic_logs::{facets, grouped, histogram, migration, page, Cursor, Filter, Governed, MagicLog};
use uuid::Uuid;

#[allow(dead_code)]
#[derive(MagicLog)]
#[log(
    name = "events",
    schema = "telemetry",
    view_schema = "api",
    segment_by = "unit_id",
    search_with = "f_unaccent"
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
    #[log(level, sql = "severity")]
    severity: String,
    #[log(facet, per_top, indexed)]
    kind: String,
    #[log(top)]
    unit_id: Uuid,
    #[log(filter)]
    operator_id: Option<Uuid>,
    #[log(search)]
    description: String,
}

/// A host's governance: a view narrowed by the reader's zones.
fn govern(g: &Governed) -> String {
    format!(
        "SELECT setup_governed_view('{}', '{}', '{}', '{}', {});",
        g.view_schema,
        g.view,
        g.schema,
        g.relation,
        g.scope.map_or("NULL".to_owned(), |s| format!("'{s}'"))
    )
}

fn day() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()
}

fn queries() -> String {
    let spec = Event::SPEC;
    let week = Filter::new(day(), day() + Duration::days(7));
    let hours = Filter::new(day(), day() + Duration::hours(6));
    let mut out = String::new();
    let mut add = |title: &str, sql: String| {
        out.push_str(&format!("-- {title}\n{sql};\n\n"));
    };
    let cursor = Cursor {
        time: day(),
        id: Uuid::nil().to_string(),
    };

    add(
        "page",
        page(&spec, &week, None, 100).unwrap().sql().to_owned(),
    );
    add(
        "page after a cursor, narrowed and searched",
        page(
            &spec,
            &week.clone().with("kind", ["door"]).search("gate"),
            Some(&cursor),
            100,
        )
        .unwrap()
        .sql()
        .to_owned(),
    );
    add(
        "histogram by day: the daily counts",
        histogram(&spec, &week, Duration::days(1))
            .unwrap()
            .sql()
            .to_owned(),
    );
    add(
        "histogram by hour, narrowed to counted fields: the hourly counts",
        histogram(
            &spec,
            &hours
                .clone()
                .with("kind", ["door"])
                .with("severity", ["Error"]),
            Duration::hours(1),
        )
        .unwrap()
        .sql()
        .to_owned(),
    );
    add(
        "histogram by five minutes: the rows",
        histogram(&spec, &hours, Duration::minutes(5))
            .unwrap()
            .sql()
            .to_owned(),
    );
    add(
        "histogram narrowed to a field the counts do not keep: the rows",
        histogram(
            &spec,
            &week.clone().with("operator_id", [Uuid::nil().to_string()]),
            Duration::days(1),
        )
        .unwrap()
        .sql()
        .to_owned(),
    );
    add(
        "facets over a week, each under the others' filters",
        facets(
            &spec,
            &week
                .clone()
                .with("kind", ["door"])
                .with("unit_id", [Uuid::nil().to_string()]),
            &["severity", "kind", "unit_id"],
        )
        .unwrap()
        .sql()
        .to_owned(),
    );
    add(
        "facets over six hours: the hourly counts",
        facets(&spec, &hours, &["severity", "kind"])
            .unwrap()
            .sql()
            .to_owned(),
    );
    add(
        "grouped: the counts per day and top",
        grouped(&spec, &week, "kind", 50).unwrap().sql().to_owned(),
    );
    add(
        "grouped, searched: the rows",
        grouped(&spec, &week.clone().search("gate"), "kind", 50)
            .unwrap()
            .sql()
            .to_owned(),
    );
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

fn golden(file: &str, made: &str) {
    let path = format!("{}/tests/golden/{file}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("MAGIC_LOGS_BLESS").is_some() {
        std::fs::write(&path, made).unwrap();
        return;
    }
    let kept = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        kept == made,
        "{file} is not what the crate writes: MAGIC_LOGS_BLESS=1 writes it anew\n\n{made}"
    );
}

#[test]
fn the_migration_is_the_one_checked_in() {
    golden("migration.sql", &migration(&Event::SPEC, govern));
}

#[test]
fn every_question_is_the_sql_checked_in() {
    golden("queries.sql", &queries());
}
