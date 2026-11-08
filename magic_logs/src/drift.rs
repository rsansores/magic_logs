//! Whether the database holds the table as its declaration says: what a
//! test runs against a migrated database, so that a table made by an older
//! migration — which a released migration file never changes — still is the
//! table the code declares.

use sqlx::PgConnection;

use crate::migration::{count_columns, indexes, refresh, segmented, sparse_index};
use crate::spec::{Spec, Tier};

/// One way the database differs from the declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    /// The relation it is of: `schema.name`, unquoted.
    pub relation: String,
    /// What differs: `chunk`, `segmentby`, `index name`, `count`, …
    pub what: String,
    /// What the declaration says.
    pub declared: String,
    /// What the database holds: `none` when it holds nothing.
    pub found: String,
}

impl std::fmt::Display for Drift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} is {}, declared {}",
            self.relation, self.what, self.found, self.declared
        )
    }
}

/// Every way the database `conn` reads differs from `spec`, none when it
/// holds the table as declared: the hypertable and its chunks, the
/// columnstore and its policy — the segments, the order and, on TimescaleDB
/// 2.22 and later, the sparse indexes —, the indexes the migration makes,
/// every count with its columns, its refresh and its columnstore, a count
/// the table should not have, and a view of every relation in
/// [`Spec::view_schema`]. An index or a view of the host's own beyond these
/// is not a difference; what a view reads is the host's.
///
/// Read by the connection's role: one that cannot see the table's schema
/// sees nothing, and is told every relation is missing.
pub async fn drift(conn: &mut PgConnection, spec: &Spec) -> Result<Vec<Drift>, sqlx::Error> {
    let mut out = Vec::new();
    let mut differs = |relation: &str, what: &str, declared: String, found: String| {
        if declared != found {
            out.push(Drift {
                relation: relation.to_owned(),
                what: what.to_owned(),
                declared,
                found,
            });
        }
    };
    let table = format!("{}.{}", spec.schema, spec.name);

    let (time, chunk): (Option<String>, Option<f64>) = sqlx::query_as(
        "SELECT (SELECT column_name::text FROM timescaledb_information.dimensions \
                  WHERE hypertable_schema = $1 AND hypertable_name = $2 AND dimension_number = 1), \
                (SELECT extract(epoch FROM time_interval)::float8 FROM timescaledb_information.dimensions \
                  WHERE hypertable_schema = $1 AND hypertable_name = $2 AND dimension_number = 1)",
    )
    .bind(spec.schema)
    .bind(spec.name)
    .fetch_one(&mut *conn)
    .await?;
    let Some(time) = time else {
        differs(&table, "hypertable", "a hypertable".into(), "none".into());
        return Ok(out);
    };
    differs(&table, "time", spec.time.to_owned(), time);
    let chunk_declared = seconds(conn, spec.storage.chunk).await?;
    differs(
        &table,
        "chunk",
        interval(chunk_declared),
        chunk.map_or("none".into(), interval),
    );
    columnstore(
        conn,
        &mut differs,
        Store {
            relation: &table,
            hypertable: (spec.schema, spec.name),
            jobs_name: (spec.schema, spec.name),
            segmentby: unquoted(&segmented(spec)),
            orderby: format!("{} DESC,{} DESC", spec.time, spec.id.name),
            sparse: sparse_index(spec).map(|s| unquoted(&s)),
            after: spec.storage.compress_after,
        },
    )
    .await?;

    let found: Vec<(String, Vec<String>, Vec<bool>)> = sqlx::query_as(
        "SELECT c.relname::text, array_agg(a.attname::text ORDER BY k.n), \
                array_agg((i.indoption[k.n - 1] & 1) = 1 ORDER BY k.n) \
           FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid \
          CROSS JOIN LATERAL unnest(i.indkey::int2[]) WITH ORDINALITY AS k(attnum, n) \
           JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum \
          WHERE i.indrelid = (SELECT oid FROM pg_class \
                               WHERE relname = $2 AND relnamespace = $1::regnamespace) \
          GROUP BY c.relname",
    )
    .bind(spec.schema)
    .bind(spec.name)
    .fetch_all(&mut *conn)
    .await?;
    for (name, columns) in indexes(spec) {
        let declared = listed(columns.iter().map(|(c, desc)| ((*c).to_owned(), *desc)));
        let held = found
            .iter()
            .find(|(n, _, _)| *n == name)
            .map_or("none".to_owned(), |(_, cols, desc)| {
                listed(cols.iter().cloned().zip(desc.iter().copied()))
            });
        differs(&table, &format!("index {name}"), declared, held);
    }

    let counts: Vec<(String, bool, String, String)> = sqlx::query_as(
        "WITH declared AS (SELECT * FROM timescaledb_information.continuous_aggregates \
                            WHERE view_schema = $1 AND view_name = ANY($3)) \
         SELECT view_name::text, materialized_only, materialization_hypertable_schema::text, \
                materialization_hypertable_name::text \
           FROM timescaledb_information.continuous_aggregates \
          WHERE (hypertable_schema, hypertable_name) IN (($1, $2)) \
             OR (hypertable_schema, hypertable_name) IN \
                (SELECT materialization_hypertable_schema, materialization_hypertable_name FROM declared) \
             OR (view_schema, view_name) IN (SELECT view_schema, view_name FROM declared)",
    )
    .bind(spec.schema)
    .bind(spec.name)
    .bind(
        spec.tiers()
            .into_iter()
            .filter(|t| *t != Tier::Raw)
            .map(|t| spec.relation(t))
            .collect::<Vec<_>>(),
    )
    .fetch_all(&mut *conn)
    .await?;
    let declared: Vec<Tier> = spec
        .tiers()
        .into_iter()
        .filter(|t| *t != Tier::Raw)
        .collect();
    for (name, ..) in &counts {
        if !declared.iter().any(|t| spec.relation(*t) == *name) {
            differs(
                &format!("{}.{name}", spec.schema),
                "count",
                "none".into(),
                "a count of the table".into(),
            );
        }
    }
    for tier in declared {
        let relation = spec.relation(tier);
        let qualified = format!("{}.{relation}", spec.schema);
        let Some((_, materialized_only, hyper_schema, hyper_name)) =
            counts.iter().find(|(n, ..)| *n == relation)
        else {
            differs(&qualified, "count", "a count".into(), "none".into());
            continue;
        };
        differs(
            &qualified,
            "materialized_only",
            "false".into(),
            materialized_only.to_string(),
        );
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT attname::text FROM pg_attribute \
              WHERE attrelid = (SELECT oid FROM pg_class \
                                 WHERE relname = $2 AND relnamespace = $1::regnamespace) \
                AND attnum > 0 AND NOT attisdropped ORDER BY attnum",
        )
        .bind(spec.schema)
        .bind(&relation)
        .fetch_all(&mut *conn)
        .await?;
        differs(
            &qualified,
            "columns",
            count_columns(spec, tier).join(","),
            columns.join(","),
        );
        let (end, every) = refresh(tier);
        let policy: Option<(Option<String>, Option<f64>, f64)> = sqlx::query_as(
            "SELECT config->>'start_offset', \
                    extract(epoch FROM (config->>'end_offset')::interval)::float8, \
                    extract(epoch FROM schedule_interval)::float8 \
               FROM timescaledb_information.jobs \
              WHERE proc_name = 'policy_refresh_continuous_aggregate' \
                AND hypertable_schema = $1 AND hypertable_name = $2",
        )
        .bind(spec.schema)
        .bind(&relation)
        .fetch_optional(&mut *conn)
        .await?;
        let declared = format!(
            "from the first row, to {}, every {}",
            interval(seconds(conn, end).await?),
            interval(seconds(conn, every).await?)
        );
        let held = policy.map_or("none".to_owned(), |(start, end, every)| {
            format!(
                "from {}, to {}, every {}",
                start.unwrap_or_else(|| "the first row".to_owned()),
                end.map_or("now".to_owned(), interval),
                interval(every)
            )
        });
        differs(&qualified, "refresh", declared, held);
        let chunk: Option<f64> = sqlx::query_scalar(
            "SELECT extract(epoch FROM time_interval)::float8 \
               FROM timescaledb_information.dimensions \
              WHERE hypertable_schema = $1 AND hypertable_name = $2 AND dimension_number = 1",
        )
        .bind(hyper_schema)
        .bind(hyper_name)
        .fetch_optional(&mut *conn)
        .await?;
        differs(
            &qualified,
            "chunk",
            interval(chunk_declared),
            chunk.map_or("none".into(), interval),
        );
        columnstore(
            conn,
            &mut differs,
            Store {
                relation: &qualified,
                hypertable: (hyper_schema, hyper_name),
                jobs_name: (spec.schema, &relation),
                segmentby: String::new(),
                orderby: "bucket".to_owned(),
                sparse: None,
                after: spec.storage.compress_after,
            },
        )
        .await?;
    }

    for tier in spec.tiers() {
        let relation = spec.relation(tier);
        let view: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_class \
                             WHERE relname = $2 AND relnamespace = $1::regnamespace AND relkind = 'v')",
        )
        .bind(spec.view_schema)
        .bind(&relation)
        .fetch_one(&mut *conn)
        .await?;
        differs(
            &format!("{}.{relation}", spec.view_schema),
            "view",
            "a view".into(),
            if view { "a view" } else { "none" }.into(),
        );
    }
    Ok(out)
}

/// What a hypertable's columnstore is declared: the relation it is
/// reported as, the hypertable, its segments and order, its sparse indexes,
/// and how old a chunk is compressed.
struct Store<'a> {
    relation: &'a str,
    hypertable: (&'a str, &'a str),
    /// What TimescaleDB's jobs name it by: the hypertable, or a count's view.
    jobs_name: (&'a str, &'a str),
    segmentby: String,
    orderby: String,
    sparse: Option<String>,
    after: &'a str,
}

/// A columnstore's settings, as the catalog holds them.
#[derive(sqlx::FromRow)]
struct Settings {
    segments: Option<String>,
    orderby: Option<String>,
    /// Whether the release declares sparse indexes: 2.22 and later.
    declares_sparse: bool,
    /// The sparse indexes declared, `kind(column)`, comma-separated.
    sparse: Option<String>,
}

/// The columnstore `store` declares, against the database's: the segments,
/// the order, the sparse indexes when the release declares them, and the
/// policy that compresses it.
async fn columnstore(
    conn: &mut PgConnection,
    differs: &mut impl FnMut(&str, &str, String, String),
    store: Store<'_>,
) -> Result<(), sqlx::Error> {
    let (schema, name) = store.hypertable;
    // The sparse indexes are a column of the settings from 2.22 on: read off
    // the row, so a release without them answers none.
    let settings: Option<Settings> = sqlx::query_as(
        "SELECT s.segmentby AS segments, s.orderby, to_jsonb(s) ? 'index' AS declares_sparse, \
                (SELECT string_agg((e->>'type') || '(' || (e->>'column') || ')', ',' ORDER BY n) \
                   FROM jsonb_array_elements(CASE jsonb_typeof(to_jsonb(s) -> 'index') \
                                             WHEN 'array' THEN to_jsonb(s) -> 'index' ELSE '[]' END) \
                        WITH ORDINALITY AS i(e, n) \
                  WHERE e->>'source' = 'config') AS sparse \
           FROM timescaledb_information.hypertable_columnstore_settings s \
          WHERE s.hypertable = (SELECT oid FROM pg_class \
                                 WHERE relname = $2 AND relnamespace = $1::regnamespace)::regclass",
    )
    .bind(schema)
    .bind(name)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(Settings {
        segments,
        orderby,
        declares_sparse,
        sparse,
    }) = settings
    else {
        differs(
            store.relation,
            "columnstore",
            "enabled".into(),
            "none".into(),
        );
        return Ok(());
    };
    differs(
        store.relation,
        "segmentby",
        store.segmentby,
        unquoted(&segments.unwrap_or_default()),
    );
    differs(
        store.relation,
        "orderby",
        store.orderby,
        unquoted(&orderby.unwrap_or_default()).replace(" NULLS FIRST", ""),
    );
    if declares_sparse {
        differs(
            store.relation,
            "sparse_index",
            store.sparse.unwrap_or_default(),
            sparse.unwrap_or_default(),
        );
    }
    let policy: Option<f64> = sqlx::query_scalar(
        "SELECT extract(epoch FROM (config->>'compress_after')::interval)::float8 \
           FROM timescaledb_information.jobs \
          WHERE proc_name = 'policy_compression' AND hypertable_schema = $1 AND hypertable_name = $2",
    )
    .bind(store.jobs_name.0)
    .bind(store.jobs_name.1)
    .fetch_optional(&mut *conn)
    .await?;
    let declared = seconds(conn, store.after).await?;
    differs(
        store.relation,
        "compress_after",
        interval(declared),
        policy.map_or("none".into(), interval),
    );
    Ok(())
}

/// `text`, an interval, in seconds, as the database reads it.
async fn seconds(conn: &mut PgConnection, text: &str) -> Result<f64, sqlx::Error> {
    sqlx::query_scalar("SELECT extract(epoch FROM $1::interval)::float8")
        .bind(text)
        .fetch_one(conn)
        .await
}

/// Seconds, as an interval reads: `1 day`, `2 hours`, `90 seconds`.
fn interval(seconds: f64) -> String {
    let whole = seconds.round() as i64;
    for (unit, size) in [("day", 86_400), ("hour", 3_600), ("minute", 60)] {
        if whole != 0 && whole % size == 0 {
            let n = whole / size;
            return format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
        }
    }
    format!("{whole} seconds")
}

/// A list of SQL identifiers as the catalog spells them: without quotes, and
/// no space after a comma.
fn unquoted(text: &str) -> String {
    text.replace('"', "").replace(", ", ",")
}

/// Columns, each with its order, as one line.
fn listed(columns: impl Iterator<Item = (String, bool)>) -> String {
    columns
        .map(|(c, desc)| if desc { format!("{c} DESC") } else { c })
        .collect::<Vec<_>>()
        .join(",")
}
