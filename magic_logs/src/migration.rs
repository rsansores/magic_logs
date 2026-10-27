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
/// `relation` in `schema`, narrowed by `scope` when there is one. The names
/// are as the table spells them, unquoted; the `quoted_` methods give them
/// as SQL must write them.
#[derive(Debug, Clone, Copy)]
pub struct Governed<'a> {
    /// The schema the view is made in: [`Spec::view_schema`](crate::Spec::view_schema).
    pub view_schema: &'a str,
    /// The view's name: the relation's own.
    pub view: &'a str,
    /// The schema the relation lives in: [`Spec::schema`](crate::Spec::schema).
    pub schema: &'a str,
    /// The table, or one of its counts.
    pub relation: &'a str,
    /// The column a reader is narrowed by, kept in every relation.
    pub scope: Option<&'a str>,
}

impl Governed<'_> {
    /// The view, schema-qualified and quoted: what `CREATE VIEW` names.
    pub fn quoted_view(&self) -> String {
        qualified(self.view_schema, self.view)
    }

    /// The relation the view reads, schema-qualified and quoted.
    pub fn quoted_relation(&self) -> String {
        qualified(self.schema, self.relation)
    }

    /// The scope column, quoted.
    pub fn quoted_scope(&self) -> Option<String> {
        self.scope.map(ident)
    }
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
         counted by hour{counts}.",
        table = qualified(spec.schema, spec.name),
        chunk = st.chunk,
        after = st.compress_after,
        counts = spec.top.map_or(" and by day".to_owned(), |t| format!(
            ", by day, and by day and {}",
            t.name
        )),
    ));
    push(format!(
        "SELECT create_hypertable({}, by_range({}, INTERVAL {}),\n                         create_default_indexes => false, migrate_data => true);",
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

    // No segment_by is no segmenting, said: left out, TimescaleDB picks a
    // column of its own, the id among them, which the order already uses.
    let segment = format!(
        ",\n    timescaledb.segmentby = {}",
        literal(&st.segment_by.map(ident).unwrap_or_default())
    );
    push(format!(
        "ALTER TABLE {table} SET (\n    timescaledb.enable_columnstore{segment},\n    timescaledb.orderby = {}\n);\nCALL add_columnstore_policy({}, after => INTERVAL {});",
        literal(&format!("{time} DESC, {id} DESC")),
        literal(&table),
        literal(st.compress_after)
    ));

    // `, "a", "b"`: nothing at all for a table that counts only its rows.
    let listed = |columns: Vec<&str>| -> String {
        columns.iter().map(|c| format!(", {}", ident(c))).collect()
    };
    let counted = listed(spec.counted());
    let hourly = qualified(spec.schema, &spec.relation(Tier::Hourly));
    push(format!(
        "CREATE MATERIALIZED VIEW {hourly}\nWITH (timescaledb.continuous, timescaledb.materialized_only = false) AS\n\
         SELECT time_bucket(INTERVAL '1 hour', {time}) AS bucket{counted}, count(*) AS events\n  FROM {table}\n GROUP BY 1{counted}\nWITH NO DATA;",
    ));
    let daily = qualified(spec.schema, &spec.relation(Tier::Daily));
    push(format!(
        "CREATE MATERIALIZED VIEW {daily}\nWITH (timescaledb.continuous, timescaledb.materialized_only = false) AS\n\
         SELECT time_bucket(INTERVAL '1 day', bucket) AS bucket{counted}, sum(events) AS events\n  FROM {hourly}\n GROUP BY 1{counted}\nWITH NO DATA;",
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
        let per_top = listed(spec.counted_per_top());
        let daily_top = qualified(spec.schema, &spec.relation(Tier::DailyTop));
        push(format!(
            "CREATE MATERIALIZED VIEW {daily_top}\nWITH (timescaledb.continuous, timescaledb.materialized_only = false) AS\n\
             SELECT time_bucket(INTERVAL '1 day', {time}) AS bucket{per_top},\n       count(*) AS events, min({time}) AS first_at, max({time}) AS last_at\n  FROM {table}\n GROUP BY 1{per_top}\nWITH NO DATA;",
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
