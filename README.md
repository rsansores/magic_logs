# magic_logs

One PostgreSQL + TimescaleDB time-series table, and the questions a log
explorer asks of it — a page of rows, a histogram, facets with counts,
grouped counts — each answered from the cheapest table that can.

```rust
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
```

And [`magic_logs_explorer`](explorer/), the Vue 3 side without its looks:
the window, the facets, the histogram's geometry and its brush, the pages
— state in your URL, drawn by your own components.

![The explorer's demo: a day of a fleet's events, an incident in the harbor](https://raw.githubusercontent.com/rsansores/magic_logs/main/explorer/images/explorer.png)

## Requirements

- PostgreSQL 14 or later, with TimescaleDB 2.18 or later under its
  community license (the columnstore and continuous aggregates are not in
  the Apache-2.0-only build), preloaded.
- Rust 1.85 or later; sqlx 0.8.
- The table, already there with its columns and a primary key that holds
  the time column (a hypertable's unique keys must). Rows it already holds
  are moved into chunks by the migration, which holds the table locked for
  as long as that takes.

## Why

A table of events grows by millions of rows a day and is read as an
explorer reads it: how many, of what, when, then the rows themselves. Read
from the rows, every one of those counts scans the window; a month is
hundreds of millions of rows, and a dashboard that counts them takes the
database with it. Paged by offset, the last page reads every page before
it. Kept forever, the table outgrows its disk.

magic_logs makes the table a TimescaleDB hypertable of one-day chunks,
compressed after two days (about twelve to one), with counts kept by hour,
by day, and by day and entity, refreshed as rows arrive — late ones
included. Each question reads the smallest table that answers it: a month's
histogram reads thirty rows per value, not a billion. A page is keyset,
newest first, from an index.

## What it does, and what it does not

- **One table, counts only.** No joins, no sums or percentiles, no query
  language, no alerting. A question names a window, values of the table's
  own fields, and a search.
- **No authorization of its own, and every read governed.** Each relation —
  the rows and every count — is read through a view your SQL writes (the
  `govern` function the migration calls), in a schema of its own. Row-level
  security cannot sit on a table with a columnstore or under a continuous
  aggregate, so the views are where your rules go; the `scope` column (a
  tenant's district, a team, a zone) is kept in every count so a view can
  narrow each.
- **Counts are of whole buckets.** A window that begins mid-hour counts that
  hour from the hourly counts, and a week from the daily counts begins at
  midnight UTC. An explorer shows them rounded; the rows themselves are
  always exact. A question that names a field the counts do not keep, a
  search, or a step finer than an hour reads the rows.
- **Late rows count.** The counts are refreshed over all of history: a
  refresh with nothing new costs milliseconds, and a row that arrives days
  late recounts only the buckets it fell in.
- **No retention.** Dropping old chunks is one TimescaleDB call
  (`add_retention_policy`), and yours to make.

## Defining a table

```rust
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
```

| on | key | means |
|---|---|---|
| struct | `name`, `schema` | the table, in a schema only your own work reads (required) |
| struct | `view_schema` | where the readable views live; `public` when left out |
| struct | `chunk`, `compress_after` | `1 day` and `2 days` when left out |
| struct | `segment_by` | the column a compressed chunk keeps rows together by: the entity most pages are narrowed to |
| struct | `search_with` | a function both sides of a search pass through (`f_unaccent`, `lower`) |
| struct | `crate` | the path to magic_logs, when you re-export it |
| field | `id`, `time` | the tie-breaker and the time (required) |
| field | `tenant`, `scope` | kept in every count; `scope` is what your views narrow readers by |
| field | `level` | counted, and what the histogram is stacked by |
| field | `facet` | counted by hour and by day; `per_top` also per day and `top`; `indexed` (of a `facet` or a `filter`) gets an index of its own |
| field | `top` | the entity counted on its own, per day: the grouped view's rows |
| field | `filter` | a filter the counts do not keep: naming it reads the rows |
| field | `search` | searched as text, every such column |
| field | `sql = "…"` | the SQL type a filter's values are cast to, when the Rust type does not say (an enum) |

A field with no `#[log(…)]` is still read by a page (it is a column of the
row), and nothing else. The derive refuses what the migration could not
make: `schema` and `view_schema` the same (a view over itself), a counted
column named `bucket`, `events`, `first_at` or `last_at` (the counts' own),
and a relation or index name past PostgreSQL's 63 bytes. A field named
`r#type` is the column `type`.

## Setting up

`migration` prints the SQL, one transaction's worth, for the table as it
stands: the hypertable, the indexes a page reads (`time DESC, id DESC`,
and one per `indexed` field and the `top`), the columnstore and its
policy, the three counts and their refresh policies — and, for each of
the four relations, whatever `govern` writes. Run it from your own
migrations; the [golden copy](magic_logs/tests/golden/migration.sql) shows
what it writes.

```rust
use magic_logs::Governed;

/// Every relation readable through a view that keeps the reader's zones.
fn govern(g: &Governed) -> String {
    let scope = g.quoted_scope().unwrap_or_else(|| "zone".to_owned());
    format!(
        "CREATE VIEW {} AS SELECT * FROM {} \
          WHERE {scope} = ANY((SELECT string_to_array(current_setting('app.zones', true), ','))::text[]);",
        g.quoted_view(),
        g.quoted_relation(),
    )
}
```

Write the predicate so the planner evaluates it once per query and can run
it in parallel workers: a setting read as `(SELECT current_setting(…))`, a
function marked `STABLE PARALLEL SAFE`. Measured on a month of hourly
counts, four facets took 4.1 s through a predicate evaluated per row in one
worker, and 0.46 s through one evaluated once, in parallel
([how it works](HOW_IT_WORKS.md#governing)).

## Asking

Each question returns a `sqlx::QueryBuilder` for you to run in your own
transaction — where the settings your views read live.

```rust
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
```

| question | reads |
|---|---|
| `page` | the rows, from `(time DESC, id DESC)`, or a field's own index |
| `histogram` | the daily counts for a step of whole days, the hourly ones for whole hours, when the filter names only counted fields; the rows otherwise |
| `facets` | the daily counts past three days, the hourly ones within; a `top` from the counts per day and `top` past three days; the rows for what the counts do not keep |
| `grouped` | the counts per day and `top` past three days, when the filter and the grouped field are kept there; the rows otherwise |

A refused question — a window that ends before it begins, a field the table
does not have, one named twice, a search of a table that searches nothing,
a step that is not positive, a negative limit — is an
[`Error`](https://docs.rs/magic_logs/latest/magic_logs/enum.Error.html),
before any SQL is written: answer it as the caller's mistake. A value is
sent as text and cast by the database to the column's type: one that does
not read (`not-a-uuid` for a `uuid`) is refused there, SQLSTATE `22P02`,
which is the caller's mistake too. A search is text as typed: `%` and `_`
match themselves.

With the `serde` feature, `Bucket`, `FacetCount` and `Group` serialize as
`magic_logs_explorer` reads them: return them as they are.

## Performance

Measured in the first host, 2026-10-26: 300 million events (2,000 units ×
5,000 events a day × 30 days), PostgreSQL 18.6 with TimescaleDB 2.31, in a
container of 8 CPUs and 10 GB; each read through a governed view, p50 of
five warm runs.

| question, 30 days | ms |
|---|---|
| a page of 100, newest first | 2 (8 for a reader of 2 % of the units) |
| the histogram, by day | 17 |
| four facets | 33 |
| the 50 largest groups | ~300 |
| a page narrowed to one kind and level | 2–70 |

A day of rows is 4.2 GB before compression; the thirty days hold 17 GB in
all, two uncompressed. What stays slow: a one-hour window at one-minute
bars reads the rows (~270 ms), and a page deep inside compressed history
decompresses its chunk (~2 s). [How it works](HOW_IT_WORKS.md#what-it-costs)
has the rest, and what each number was before the fix it measures.

## Learn more

- [How it works](HOW_IT_WORKS.md): the tables, the counts and their
  refresh, which question reads what, governing, what it costs.
- [`magic_logs_explorer`](explorer/): the Vue 3 explorer, without its looks,
  and its demo.
- [`examples/fleet`](examples/fleet/): a server over TimescaleDB, its reads
  governed by zone, that the demo reads.
- [The changelog](CHANGELOG.md), and [how to contribute](CONTRIBUTING.md).

## Status

0.1: the first release. Until 1.0 a minor version may break the API, and
the changelog says how; the SQL the migration writes changes only with a
version that says what to run on a table made by the one before.

## License

MIT or Apache-2.0, at your option.
