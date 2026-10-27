# Changelog

## 0.1.0 — unreleased

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
  - TimescaleDB 2.18 or later on PostgreSQL 14 or later; Rust 1.85.
- `magic_logs_explorer` (npm): the Vue 3 explorer without its looks —
  `useLogExplorer` (the window, the chosen values, the search, live, the
  pages; in memory, the address bar or the host's router), `useLogFetch`,
  and the renderless `LogExplorerRoot`, `LogHistogram`, `LogFacet`,
  `LogRows` and `LogGroups`; `magic_logs_explorer/core` without a
  framework.
- `examples/fleet`: a server over TimescaleDB, its reads governed by zone.
