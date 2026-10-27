//! The SQL that makes an existing table a log table: a hypertable, compressed
//! as it ages, with its counts and their refresh, and every relation behind
//! the host's governance.

use crate::spec::{Spec, Tier};

/// Quotes `name` as SQL quotes an identifier.
pub(crate) fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// `schema.relation`, each quoted.
pub(crate) fn qualified(schema: &str, relation: &str) -> String {
    format!("{}.{}", ident(schema), ident(relation))
}

fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// A relation the host makes readable: `view` in `view_schema`, over
/// `relation` in `schema`, narrowed by `scope` when there is one.
#[derive(Debug, Clone, Copy)]
pub struct Governed<'a> {
    pub view_schema: &'a str,
    pub view: &'a str,
    pub schema: &'a str,
    pub relation: &'a str,
    pub scope: Option<&'a str>,
}

/// The migration for `spec`'s table, which already exists in its schema
/// with its columns and primary key (the time column in it): one statement a
/// line group, runnable in one transaction. `govern` writes the statement
/// that makes each relation readable — the raw table and every count — for
/// the host's authorization; magic_logs owns none.
///
/// The counts are refreshed over all of history (`start_offset => NULL`): a
/// refresh with nothing new costs milliseconds, and a late row recounts only
/// the buckets it fell in, however old.
pub fn migration(spec: &Spec, govern: impl Fn(&Governed) -> String) -> String {
    let table = qualified(spec.schema, spec.name);
    let time = ident(spec.time);
    let id = ident(spec.id.name);
    let st = &spec.storage;
    let mut out = String::new();
    let mut push = |s: String| {
        out.push_str(&s);
        out.push_str("\n\n");
    };

    push(format!(
        "-- magic_logs: {table}, a hypertable of {chunk} chunks compressed after {after}, \
         counted by hour, by day and by day and {top}.",
        table = qualified(spec.schema, spec.name),
        chunk = st.chunk,
        after = st.compress_after,
        top = spec.top.map_or("nothing else", |t| t.name),
    ));
    push(format!(
        "SELECT create_hypertable({}, by_range({}, INTERVAL {}),\n                         create_default_indexes => false);",
        literal(&table),
        literal(spec.time),
        literal(st.chunk)
    ));

    // A page is the newest rows first, the id breaking ties; narrowed to one
    // value of an indexed field, or to one `top`, it walks only those rows.
    let mut indexes = vec![format!(
        "CREATE INDEX {} ON {table} ({time} DESC, {id} DESC);",
        ident(&format!("{}_{}_{}_idx", spec.name, spec.time, spec.id.name))
    )];
    let indexed = spec.top.map(|t| t.name).into_iter().chain(
        spec.fields
            .iter()
            .filter(|f| f.indexed)
            .map(|f| f.column.name),
    );
    let mut seen = Vec::new();
    for field in indexed {
        if seen.contains(&field) {
            continue;
        }
        seen.push(field);
        indexes.push(format!(
            "CREATE INDEX {} ON {table} ({}, {time} DESC, {id} DESC);",
            ident(&format!("{}_{}_{}_idx", spec.name, field, spec.time)),
            ident(field)
        ));
    }
    push(indexes.join("\n"));

    let segment = st
        .segment_by
        .map(|s| format!(",\n    timescaledb.segmentby = {}", literal(&ident(s))))
        .unwrap_or_default();
    push(format!(
        "ALTER TABLE {table} SET (\n    timescaledb.enable_columnstore{segment},\n    timescaledb.orderby = {}\n);\nCALL add_columnstore_policy({}, after => INTERVAL {});",
        literal(&format!("{time} DESC, {id} DESC")),
        literal(&table),
        literal(st.compress_after)
    ));

    let counted: Vec<String> = spec.counted().iter().map(|c| ident(c)).collect();
    let hourly = qualified(spec.schema, &spec.relation(Tier::Hourly));
    push(format!(
        "CREATE MATERIALIZED VIEW {hourly}\nWITH (timescaledb.continuous, timescaledb.materialized_only = false) AS\n\
         SELECT time_bucket(INTERVAL '1 hour', {time}) AS bucket, {cols}, count(*) AS events\n  FROM {table}\n GROUP BY bucket, {cols}\nWITH NO DATA;",
        cols = counted.join(", ")
    ));
    let daily = qualified(spec.schema, &spec.relation(Tier::Daily));
    push(format!(
        "CREATE MATERIALIZED VIEW {daily}\nWITH (timescaledb.continuous, timescaledb.materialized_only = false) AS\n\
         SELECT time_bucket(INTERVAL '1 day', bucket) AS bucket, {cols}, sum(events) AS events\n  FROM {hourly}\n GROUP BY 1, {cols}\nWITH NO DATA;",
        cols = counted.join(", ")
    ));
    let mut policies = vec![
        format!(
            "SELECT add_continuous_aggregate_policy({}, start_offset => NULL, end_offset => INTERVAL '1 hour', schedule_interval => INTERVAL '5 minutes');",
            literal(&hourly)
        ),
        format!(
            "SELECT add_continuous_aggregate_policy({}, start_offset => NULL, end_offset => INTERVAL '1 day', schedule_interval => INTERVAL '1 hour');",
            literal(&daily)
        ),
    ];
    if spec.top.is_some() {
        let per_top: Vec<String> = spec.counted_per_top().iter().map(|c| ident(c)).collect();
        let daily_top = qualified(spec.schema, &spec.relation(Tier::DailyTop));
        push(format!(
            "CREATE MATERIALIZED VIEW {daily_top}\nWITH (timescaledb.continuous, timescaledb.materialized_only = false) AS\n\
             SELECT time_bucket(INTERVAL '1 day', {time}) AS bucket, {cols},\n       count(*) AS events, min({time}) AS first_at, max({time}) AS last_at\n  FROM {table}\n GROUP BY bucket, {cols}\nWITH NO DATA;",
            cols = per_top.join(", ")
        ));
        policies.push(format!(
            "SELECT add_continuous_aggregate_policy({}, start_offset => NULL, end_offset => INTERVAL '1 hour', schedule_interval => INTERVAL '5 minutes');",
            literal(&daily_top)
        ));
    }
    push(policies.join("\n"));

    let governed: Vec<String> = spec
        .tiers()
        .into_iter()
        .map(|tier| {
            let relation = spec.relation(tier);
            govern(&Governed {
                view_schema: spec.view_schema,
                view: &relation,
                schema: spec.schema,
                relation: &relation,
                scope: spec.scope,
            })
        })
        .collect();
    push(governed.join("\n"));

    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}
