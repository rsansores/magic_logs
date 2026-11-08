# Changelog

## Unreleased

Breaking: a filter field holds a `Condition`, a histogram row's `level` is
its `stack`, `segment_by` is a list, and a table that counts nothing gets no
counts.

- **Ranges.** `Condition::{AnyOf, Between}`: a field is one of some values,
  or between two inclusive bounds, either left out. `Filter::with` is
  `AnyOf`; `Filter::with_condition` takes either. The counts answer a range
  of a field they keep. `Filter::fields` holds `(field, Condition)`.
- **Any stack.** `#[log(stack = "…")]` names the counted field the
  histogram is stacked by, the `level` when left out (`Spec::stack`);
  `histogram_by` stacks by any field, or by none, from the rows when no
  count keeps it. `Bucket.level` is `Bucket.stack`, and `histogram_tier`
  takes the stack.
- **The derive refuses** a `$` in `name` or `schema` (a migration's quoted
  block cannot hold one), and a `facet` that is also the `tenant` or the
  `scope`, which every count keeps already.
- **Tables that count nothing.** A table with no `level`, `facet` or `top`
  is stored as its rows alone: the hypertable, its indexes, its columnstore
  and its view, no counts (`Spec::counts`). Such a table made by 0.1 has
  hourly and daily counts its declaration no longer has: `drift` names
  them, and any count over them, and a host drops them (`DROP MATERIALIZED VIEW … CASCADE`, after
  its views) or keeps them unread.
- **Segments and sparse indexes.** `segment_by = "a, b"` segments by
  several columns (`Storage::segment_by` is a slice). `#[log(minmax)]` and
  `#[log(bloom)]` on a column a filter names are its sparse index
  (`Field::sparse`, `Sparse`), declared by the migration on TimescaleDB
  2.22.1 and later, beside the sparse index TimescaleDB makes of each
  indexed column (a `minmax` of a time or a `numeric`, a bloom of any
  other), which declaring any would otherwise drop. A table made by 0.1 is unchanged; one
  that adds them runs `ALTER TABLE … SET (timescaledb.segmentby = …)` and
  then, on 2.22.1 and later, `ALTER TABLE … SET (timescaledb.sparse_index =
  …)` as the migration spells it — in that order, as a column 0.1 segmented
  by refuses a sparse index until it is no longer a segment column — each
  applying to the chunks compressed from then on. A bloom filter serves a
  filter of one value on every release, and one of several values from 2.31.
- A value a filter compares is cast in place, to the column's bare type,
  when that type reads text the same in every session (a number, text, a
  uuid, a boolean), so the filter can run vectorized on compressed batches
  (on 2.31: not for `numeric`); a timestamp's or an enum's is cast once, in
  a sub-select, as before. A typmod of a `numeric`, a `varchar` or a time no
  longer rounds or cuts a bound: `numeric(15,3)` compares with the value as
  sent. A filter of one value is `=`, of several `= ANY`.
- **`drift`.** What the database holds, against the declaration: the
  hypertable, its chunks, columnstore and policy, the indexes, every count
  with its columns, refresh (oldest first included) and columnstore, a count the table should not
  have, and the views. One `Drift` per difference, for a test after a
  host's migrations: a released migration is never edited, so this, not a
  file comparison, is what keeps the database the declaration.
- What the migration writes otherwise is unchanged: a table made by 0.1
  with the same declaration is the table 0.2 declares.
- `magic_logs_explorer`: `BucketRow.stack`, a histogram's `Segment.stack`
  and its order as `stacks` (`geometry`, `useHistogram`, `LogHistogram`'s
  prop), were `level` and `levels`. Ranges: `between`, a codec of
  `min..max` either side empty; `Bounds`; `Question.ranges`;
  `useLogExplorer`'s `ranges` option (`RangeOptions`) and its `ranges` and
  `rangeNames`, cleared by `clear` and counted by `narrowed`.
- `examples/fleet`: an odometer filtered by range, the trucks' positions as
  a table that counts nothing, and `drift` at start.

## 0.1.1 — 2026-10-27

- A search matches words: a row matches when every word of the search is
  in one of its searched columns, in any order (`"gate north"` finds
  `"north gate"`). It matched the whole text as one piece before.
- What the migration writes, the rows the explorer reads and the API are
  unchanged: a host upgrades by taking the version.

## 0.1.0 — 2026-10-27

The first release.

- `magic_logs_derive`: `#[derive(MagicLog)]` — a row says which column is
  its time, id, tenant, scope, level, facets, `top`, filters and search.
- `magic_logs`:
  - `migration`: the SQL that makes the table a hypertable of one-day
    chunks, compressed after two days, with counts by hour and by day, and
    by hour and by day per `top`, refreshed over all of history; each
    relation made readable by the host's `govern`.
  - `page`, `histogram`, `facets`, `grouped`: the explorer's questions as
    `sqlx::QueryBuilder`s, each read from the cheapest tables that answer
    it. Facets and groups are exact for any window (`parts`); a
    histogram's bars are its step's buckets, whole (`histogram_tier`).
  - `serde`: the answers' rows serialize as the explorer reads them.
  - TimescaleDB 2.21 or later, not 2.22.0, on PostgreSQL 15 or later (the
    migration refuses any other); Rust 1.85.
- `magic_logs_explorer` (npm): the Vue 3 explorer without its looks —
  `useLogExplorer` (the window, the chosen values, the search, live, the
  pages; in memory, the address bar or the host's router), `useLogFetch`,
  and the renderless `LogExplorerRoot`, `LogHistogram`, `LogFacet`,
  `LogRows` and `LogGroups`; `magic_logs_explorer/core` without a
  framework.
- `examples/fleet`: a server over TimescaleDB, its reads governed by zone.
