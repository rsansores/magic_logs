# magic_logs

One PostgreSQL + TimescaleDB time-series table, and the queries a log explorer
asks of it: a page of rows, a histogram, facets with counts and grouped counts,
each read from the cheapest table that answers it.

```rust
#[derive(magic_logs::MagicLog, sqlx::FromRow)]
#[log(name = "events", schema = "telemetry", segment_by = "unit_id", search_with = "f_unaccent")]
pub struct Event {
    #[log(id)]                       pub id: Uuid,
    #[log(time)]                     pub at: DateTime<Utc>,
    #[log(tenant)]                   pub tenant_id: Uuid,
    #[log(scope)]                    pub zone_id: Uuid,
    #[log(level, sql = "severity")]  pub severity: Severity,
    #[log(facet, per_top, indexed)]  pub kind: String,
    #[log(top)]                      pub unit_id: Uuid,
    #[log(filter)]                   pub operator_id: Option<Uuid>,
    #[log(search)]                   pub description: String,
}
```

- **`migration(&Event::SPEC, govern)`** prints the SQL that makes the existing
  table a hypertable with these chunks, compressed as it ages. It keeps
  counts by hour, by day (from the hourly ones), and by day and `top`, and
  refreshes them over all of history. That costs milliseconds when nothing
  changed; a late row recounts only the buckets it fell in. For each relation
  it prints the statement `govern` writes, so every one is made readable by
  the host's own authorization. The migration runs in one transaction.
- **`page`** builds the newest rows first, after a `(time, id)` cursor, from
  an index `(time DESC, id DESC)` (plus one per `indexed` field and the `top`).
- **`histogram`** builds buckets stacked by `level`, from the daily or hourly
  counts when the step is whole days or hours and the filter names only
  counted fields, and from the rows otherwise.
- **`facets`** counts every value of each named field under every other
  field's filter but its own, in one statement.
- **`grouped`** builds the largest `top` × field groups with their first and
  last row, from the counts per day and `top`.

Each returns a `sqlx::QueryBuilder` for the host to run on its own
transaction, which is where its row-level settings live.

## Attributes

| on | key | means |
|---|---|---|
| struct | `name`, `schema` | the table, in the schema only the host's own work reads (required) |
| struct | `view_schema` | where the readable views live; `public` when left out |
| struct | `chunk`, `compress_after` | `1 day` and `2 days` when left out |
| struct | `segment_by` | the column a compressed chunk keeps rows together by |
| struct | `search_with` | a function both sides of a search pass through (`f_unaccent`) |
| field | `id`, `time` | the tie-breaker and the time (required) |
| field | `tenant`, `scope` | kept in every count; `scope` is what the host narrows readers by |
| field | `level` | counted, and what the histogram is stacked by |
| field | `facet` | counted by hour and day; `per_top` also per day and `top`; `indexed` gets an index |
| field | `top` | the entity counted on its own per day |
| field | `filter` | a filter the counts do not keep: naming it reads the rows |
| field | `search` | searched as text |
| field | `sql = "…"` | the SQL type a filter's values are cast to, when the Rust type does not say |

## Scope

Rust, PostgreSQL with TimescaleDB, one table, counts only. Not generic over
databases or aggregations, no joins, no query language, no alerting. No
authorization of its own and no web framework.

Counts are of whole buckets: a window that begins mid-hour counts that hour.
An explorer shows them rounded.

## Tests

`DATABASE_URL` to a PostgreSQL with `timescaledb` preloaded, whose user may
create databases: `cargo test`.

## License

MIT or Apache-2.0, at your option.
