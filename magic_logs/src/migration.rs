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

    let counts = match spec.top {
        _ if !spec.counts() => ", stored as its rows alone".to_owned(),
        Some(t) => format!(
            ", counted by hour and by day, and by hour and by day per {}",
            t.name
        ),
        None => ", counted by hour and by day".to_owned(),
    };
    push(format!(
        "-- magic_logs: {table}, a hypertable of {chunk} chunks compressed after {after}{counts}.",
        table = qualified(spec.schema, spec.name),
        chunk = st.chunk,
        after = st.compress_after,
    ));
    // Older releases refresh compressed counts under TimescaleDB's cap on
    // rows decompressed by one statement: past it, a late row's refresh
    // fails in the background, every time. 2.22.0 loses the buckets of a
    // refresh interrupted mid-batch, for good. Refused here, by name.
    push(
        "DO $$\nDECLARE\n    v text := (SELECT extversion FROM pg_extension WHERE extname = 'timescaledb');\n\
         BEGIN\n    IF (regexp_match(v, '^(\\d+)\\.(\\d+)'))::int[] < ARRAY[2, 21] OR v = '2.22.0' THEN\n        \
         RAISE EXCEPTION 'magic_logs needs TimescaleDB 2.21 or later, but not 2.22.0; this database has %', v;\n    \
         END IF;\nEND\n$$;"
            .to_owned(),
    );
    push(format!(
        "SELECT create_hypertable({}, by_range({}, INTERVAL {}),\n                         create_default_indexes => false, migrate_data => true);",
        literal(&table),
        literal(spec.time),
        literal(st.chunk)
    ));

    let indexes: Vec<String> = indexes(spec)
        .into_iter()
        .map(|(name, columns)| {
            let columns: Vec<String> = columns
                .iter()
                .map(|(c, desc)| format!("{}{}", ident(c), if *desc { " DESC" } else { "" }))
                .collect();
            format!(
                "CREATE INDEX {} ON {table} ({});",
                ident(&name),
                columns.join(", ")
            )
        })
        .collect();
    push(indexes.join("\n"));

    // No segment_by is no segmenting, said: left out, TimescaleDB picks a
    // column of its own, the id among them, which the order already uses.
    let segment = format!(
        ",\n    timescaledb.segmentby = {}",
        literal(&segmented(spec))
    );
    push(format!(
        "ALTER TABLE {table} SET (\n    timescaledb.enable_columnstore{segment},\n    timescaledb.orderby = {}\n);\nCALL add_columnstore_policy({}, after => INTERVAL {});",
        literal(&format!("{time} DESC, {id} DESC")),
        literal(&table),
        literal(st.compress_after)
    ));
    if let Some(sparse) = sparse_index(spec) {
        // TimescaleDB declares a column's sparse index from 2.22.1 on (2.22.0
        // is refused above); on 2.21
        // a chunk keeps only those it makes of itself, and `drift` says
        // what is missing once the database runs a release that declares
        // them.
        push(format!(
            "DO $ml$\nBEGIN\n    IF (regexp_match((SELECT extversion FROM pg_extension WHERE extname = 'timescaledb'), '^(\\d+)\\.(\\d+)'))::int[] >= ARRAY[2, 22] THEN\n        \
             EXECUTE {};\n    END IF;\nEND\n$ml$;",
            literal(&format!(
                "ALTER TABLE {table} SET (timescaledb.sparse_index = {})",
                literal(&sparse)
            ))
        ));
    }
    if !spec.counts() {
        push(govern(&Governed {
            view_schema: spec.view_schema,
            view: spec.name,
            schema: spec.schema,
            relation: spec.name,
            scope: spec.scope,
        }));
        out.truncate(out.trim_end().len());
        out.push('\n');
        return out;
    }

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
    // Oldest first: a refresh that runs in batches moves the watermark only
    // over what it has counted. Newest first, it moved it past the history
    // still to count, which then was read neither from the counts nor from
    // the rows until the last batch landed.
    let mut policies = vec![
        format!(
            "SELECT add_continuous_aggregate_policy({}, start_offset => NULL, end_offset => INTERVAL '1 hour', schedule_interval => INTERVAL '5 minutes', refresh_newest_first => false);",
            literal(&hourly)
        ),
        format!(
            "SELECT add_continuous_aggregate_policy({}, start_offset => NULL, end_offset => INTERVAL '1 day', schedule_interval => INTERVAL '1 hour', refresh_newest_first => false);",
            literal(&daily)
        ),
    ];
    if spec.top.is_some() {
        let per_top = listed(spec.counted_per_top());
        let hourly_top = qualified(spec.schema, &spec.relation(Tier::HourlyTop));
        push(format!(
            "CREATE MATERIALIZED VIEW {hourly_top}\nWITH (timescaledb.continuous, timescaledb.materialized_only = false) AS\n\
             SELECT time_bucket(INTERVAL '1 hour', {time}) AS bucket{per_top},\n       count(*) AS events, min({time}) AS first_at, max({time}) AS last_at\n  FROM {table}\n GROUP BY 1{per_top}\nWITH NO DATA;",
        ));
        let daily_top = qualified(spec.schema, &spec.relation(Tier::DailyTop));
        push(format!(
            "CREATE MATERIALIZED VIEW {daily_top}\nWITH (timescaledb.continuous, timescaledb.materialized_only = false) AS\n\
             SELECT time_bucket(INTERVAL '1 day', bucket) AS bucket{per_top},\n       sum(events) AS events, min(first_at) AS first_at, max(last_at) AS last_at\n  FROM {hourly_top}\n GROUP BY 1{per_top}\nWITH NO DATA;",
        ));
        policies.push(format!(
            "SELECT add_continuous_aggregate_policy({}, start_offset => NULL, end_offset => INTERVAL '1 hour', schedule_interval => INTERVAL '5 minutes', refresh_newest_first => false);",
            literal(&hourly_top)
        ));
        policies.push(format!(
            "SELECT add_continuous_aggregate_policy({}, start_offset => NULL, end_offset => INTERVAL '1 day', schedule_interval => INTERVAL '1 hour', refresh_newest_first => false);",
            literal(&daily_top)
        ));
    }
    push(policies.join("\n"));

    // The counts compress as the rows do: the hourly per `top` hold a fifth
    // as many rows as the table. A late row's refresh writes into their
    // compressed chunks too.
    let compressed: Vec<String> = spec
        .tiers()
        .into_iter()
        .filter(|t| *t != Tier::Raw)
        .map(|tier| {
            let count = qualified(spec.schema, &spec.relation(tier));
            // Chunks as long as the table's, and compressed in one ordered
            // run per chunk: a late row's refresh rewrites the batches
            // holding its bucket, not ten days of counts. Said, not left to
            // TimescaleDB, whose defaults differ from one release to the
            // next.
            format!(
                "SELECT set_chunk_time_interval({}, INTERVAL {});\n\
                 ALTER MATERIALIZED VIEW {count} SET (\n    timescaledb.enable_columnstore = true,\n    \
                 timescaledb.segmentby = '',\n    timescaledb.orderby = 'bucket'\n);\n\
                 CALL add_columnstore_policy({}, after => INTERVAL {});",
                literal(&count),
                literal(st.chunk),
                literal(&count),
                literal(st.compress_after)
            )
        })
        .collect();
    push(compressed.join("\n"));

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

/// The table's `segmentby`: its segment_by columns, quoted, in order.
pub(crate) fn segmented(spec: &Spec) -> String {
    spec.storage
        .segment_by
        .iter()
        .map(|c| ident(c))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The table's `sparse_index` setting: each field's, in declaration order;
/// none when no field declares one. Declaring any takes the place of those
/// TimescaleDB makes of itself for each column with an index ([`own_sparse`])
/// — so the migration declares those too, after the fields' own: the
/// `top`'s and every `indexed` field's, but the time's, the id's and a
/// segment_by column's, which the order and the segments already hold.
/// What is still lost are the ones TimescaleDB makes for an index of the
/// host's own and the blooms it makes of several columns of one index.
pub(crate) fn sparse_index(spec: &Spec) -> Option<String> {
    let mut declared: Vec<(&str, &str)> = spec
        .fields
        .iter()
        .filter_map(|f| f.sparse.map(|kind| (kind.name(), f.column.name)))
        .collect();
    if declared.is_empty() {
        return None;
    }
    let indexed = spec
        .top
        .into_iter()
        .chain(spec.fields.iter().filter(|f| f.indexed).map(|f| f.column));
    for column in indexed {
        let held = column.name == spec.time
            || column.name == spec.id.name
            || spec.storage.segment_by.contains(&column.name)
            || declared.iter().any(|(_, c)| *c == column.name);
        if !held {
            declared.push((own_sparse(column.sql), column.name));
        }
    }
    Some(
        declared
            .iter()
            .map(|(kind, column)| format!("{kind}({})", ident(column)))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// The sparse index TimescaleDB makes of itself for a column with an index,
/// as 2.21 to 2.31 make them: `minmax` for a time, a date, a `numeric`, a
/// `smallint`, a float, a boolean and a type with no hash (`money`, `bit`,
/// `tsvector`); a bloom filter for the rest (`integer`, `bigint`, text, a
/// uuid, an enum, an interval). A test holds the common types to what the
/// release makes.
fn own_sparse(sql: &str) -> &'static str {
    let lower = sql.trim().to_ascii_lowercase();
    let bare = lower.strip_prefix("pg_catalog.").unwrap_or(&lower);
    let base = bare.split('(').next().unwrap_or(bare).trim();
    let ordered = base.starts_with("timestamp")
        || base.starts_with("time ")
        || matches!(
            base,
            "time"
                | "timetz"
                | "date"
                | "numeric"
                | "decimal"
                | "smallint"
                | "int2"
                | "real"
                | "float4"
                | "double precision"
                | "float8"
                | "float"
                | "boolean"
                | "bool"
                // Ordered, with no hash to build a bloom filter of.
                | "money"
                | "bit"
                | "varbit"
                | "bit varying"
                | "tsvector"
                | "\"char\""
        );
    if ordered {
        "minmax"
    } else {
        "bloom"
    }
}

/// The indexes the migration makes, each its name and its columns, `true`
/// for descending. A page is the newest rows first, the id breaking ties;
/// narrowed to one value of an indexed field, or to one `top`, it walks only
/// those rows.
pub(crate) fn indexes(spec: &Spec) -> Vec<(String, Vec<(&'static str, bool)>)> {
    let mut out = vec![(
        format!("{}_{}_{}_idx", spec.name, spec.time, spec.id.name),
        vec![(spec.time, true), (spec.id.name, true)],
    )];
    let indexed = spec.top.map(|t| t.name).into_iter().chain(
        spec.fields
            .iter()
            .filter(|f| f.indexed)
            .map(|f| f.column.name),
    );
    for field in indexed {
        let name = format!("{}_{}_{}_idx", spec.name, field, spec.time);
        if out.iter().any(|(n, _)| *n == name) {
            continue;
        }
        out.push((
            name,
            vec![(field, false), (spec.time, true), (spec.id.name, true)],
        ));
    }
    out
}

/// What a count keeps, in order: its bucket, the columns it is kept by, and
/// what it counts.
pub(crate) fn count_columns(spec: &Spec, tier: Tier) -> Vec<&'static str> {
    let mut out = vec!["bucket"];
    match tier {
        Tier::Raw => return Vec::new(),
        Tier::Hourly | Tier::Daily => {
            out.extend(spec.counted());
            out.push("events");
        }
        Tier::HourlyTop | Tier::DailyTop => {
            out.extend(spec.counted_per_top());
            out.extend(["events", "first_at", "last_at"]);
        }
    }
    out
}

/// How a count is refreshed: its `end_offset` and how often.
pub(crate) fn refresh(tier: Tier) -> (&'static str, &'static str) {
    match tier {
        Tier::Daily | Tier::DailyTop => ("1 day", "1 hour"),
        _ => ("1 hour", "5 minutes"),
    }
}
