# How it works

magic_logs writes SQL and nothing else: a migration, once, and a query per
question. It holds no connection, runs no task and keeps no state. What
runs in the background — compression, the counts' refresh — is
TimescaleDB's own job scheduler, set up by the migration. The
[golden files](magic_logs/tests/golden/) hold, word for word, what it
writes for a table with every attribute.

## The tables

The host's table becomes a hypertable: one chunk per `chunk` (a day), each
a table of its own with its own indexes. The migration creates its indexes
with `create_default_indexes => false`, and makes its own:

- `(time DESC, id DESC)`: a page, newest first. The id breaks ties between
  rows of one instant, so a page ends at one row and the next begins after
  it.
- `(field, time DESC, id DESC)` for the `top` and every `indexed` field: a
  page narrowed to one unit, or to a rare kind, walks only those rows.

A chunk older than `compress_after` (two days) is compressed into the
columnstore, ordered by `time DESC, id DESC` and segmented by `segment_by`
(the entity most pages narrow to): about twelve to one on the first host's
events, and a compressed chunk is read in that order without sorting.

Continuous aggregates — the counts — read it, two families of two:

| relation | bucket | kept by | read for |
|---|---|---|---|
| `<name>_hourly` | an hour | the tenant, the scope, the level, every `facet` | hours |
| `<name>_daily` | a day, from the hourly | the same | days |
| `<name>_hourly_<top>` | an hour | the tenant, the `top`, the scope, the level, every `per_top` facet; first and last time | a question naming the `top`, grouped |
| `<name>_daily_<top>` | a day, from the hourly per `top` | the same | the same, by day |

A `top` ending in `_id` names its counts without it: `unit_id` gives
`events_hourly_unit`. Each daily count is a continuous aggregate over its
hourly one, so a day is summed from 24 rows, not from its events.

## The refresh

Every count is refreshed over all of history (`start_offset => NULL`): the
hourly ones every five minutes up to an hour ago, the daily ones every
hour up to a day ago. TimescaleDB keeps a log of which buckets
changed since the last refresh, so a refresh with nothing new costs
milliseconds (about 30 ms on the first host's 300 million rows), and one
after a late row recounts only the buckets that row fell in, however old.
What lies past the counts' watermark — the end of the last refresh — is
read from the rows (`materialized_only = false`): a count always includes
the last hour. A policy refreshes a long history in batches, oldest first,
so the watermark only ever moves over what is counted (TimescaleDB's
default is newest first, which moved it past history still to count:
after a migration over existing rows, every answer came up short until
the last batch landed).

A refresh of your own (after a backfill, say) ends where the policy's
would: `CALL refresh_continuous_aggregate('…_hourly', NULL, now() -
interval '1 hour')`, the daily one at `now() - interval '1 day'` — each
hourly count before its daily one, the pair per `top` too. While a policy
job refreshes the same count, yours fails with SQLSTATE `55P03`: retry it,
or hold the job first (`SELECT alter_job(job_id, scheduled => false)`) and
let it go after. One that ends later (`NULL`, or in the future) moves the watermark
past now: a row that arrives in a bucket already counted is not read from
the rows, and waits for the policy to reach that bucket — up to two days
for the daily counts.

What a late row costs, measured: 1,000 rows sixty days late, into a new
chunk, 30 ms; 1,000 rows into a compressed day twenty days back, 13 s, the
whole day recounted; a backlog of 105,000 rows spread over twenty-one
days, about six minutes. That runs in the background job, not in a reader's
request, and a fleet of devices that reconnect after weeks keeps the job
busy rather than the readers waiting. No window is cut off: a late row is
counted by the next refresh of each count — within five minutes for the
hourly and per-`top` counts, within the hour for the daily — and until then
the counts of its bucket are short of the rows by it. A row past the
watermark is counted at once (real-time aggregation).

## Which question reads what

A question names a window, values of some fields, and maybe a search. A
search, or a field no count keeps (a `filter`), reads the rows. Otherwise
the counts answer, from the family that keeps every field named: the
hourly and daily counts, or — when the question names the `top`, or
groups by it — those per `top`.

- A **histogram** (`histogram_tier`) reads the daily counts for a step of
  whole days, the hourly ones for whole hours, and the rows for a finer
  step. It counts every bucket of its step the window touches, whole:
  `[time_bucket(step, from), time_bucket(step, to − 1µs) + step)`, the same
  bounds on the counts and on the rows, so each bar is exactly its
  bucket's count whichever it reads.
- A **facet** and a **group** are exact: the window is split into
  [`parts`](https://docs.rs/magic_logs/latest/magic_logs/fn.parts.html) —
  the whole days inside it from the daily counts, the whole hours beside
  them from the hourly ones, the minutes at either end from the rows — and
  the parts are summed. A window inside one hour is read from the rows
  alone. A group's first and last row are those of the window.
- A **page** always reads the rows.

A window from 10:20 on the 1st to 05:10 on the 3rd is read as 10:20–11:00
from the rows, 11:00–00:00 from the hourly counts, the 2nd from the daily
counts, 00:00–05:00 from the hourly counts and 05:00–05:10 from the rows:
what it costs is a day of daily rows, a day of hourly rows and at most two
hours of the rows themselves, whatever its length.

## Facets

A facet counts each value of one field under every other field's filter
but its own: with `kind = door` chosen, the `kind` facet still counts every
kind, so a reader sees what choosing a second would add. Each field is
summed over its own parts — its own filter left out, so its parts may read
other tables than its neighbours' — and the fields are one statement
(`UNION ALL`): one round trip.

## Pages

`page` is keyset: `WHERE time <= cursor.time AND (time, id) < (cursor)
ORDER BY time DESC, id DESC LIMIT n`. The first condition says the same as
the second, in the form TimescaleDB excludes chunks — and compressed
segments — by: without it, a page in history opens every chunk newer than
its cursor. Page fifty costs what page one costs. An id type the host's
column has (`uuid`, `bigint`) is cast from the cursor's text, so the cursor
crosses an HTTP boundary as a string. There is no total: a count of the
window is the histogram's sum.

## Governing

magic_logs owns no authorization, and the tables cannot carry it:
TimescaleDB refuses a continuous aggregate over a table with row-level
security, and refuses a columnstore on one (in either order). So every
relation is read through a view, in `view_schema`, that the host writes —
the migration calls `govern` once per relation, and only those views are
granted to the roles a request runs as. The `scope` column is kept in
every count so a view can narrow each the same way: a reader of one
district sees its district's counts, not the tenant's.

A view's predicate runs on every query, and how it is written decides
what the query costs. On the first host, four facets over a month of
hourly counts:

| predicate | ms |
|---|---|
| per row, in a function the planner may not run in parallel | 4,109 |
| once per query (`(SELECT current_setting(…))`, an InitPlan), in parallel | 459 |
| the same, from the daily counts | 33 |

Two rules follow. A setting or a function of the session is wrapped in a
scalar subquery, so it is an InitPlan evaluated once, not a call per row.
A function the predicate calls is `STABLE PARALLEL SAFE` when it is (a read
of tables and settings is), or the whole query runs in one worker.

A view written `WITH CASCADED CHECK OPTION` also judges what is written
through it, as a row-level policy without a `TO` clause would.

## What it costs

The first host, 2026-10-26: 300 million events (2,000 units × 5,000 events
a day × 30 days, 25 kinds, levels 80/15/4/1 %), PostgreSQL 18.6 with
TimescaleDB 2.31, a container of 8 CPUs and 10 GB with 2.5 GB of shared
buffers; p50 of five warm runs, milliseconds, read through governed views.
"Scoped" is a reader of one district: 48 units, 2.4 % of the rows.

| question | 1 day | 7 days | 30 days | scoped, 30 days |
|---|---|---|---|---|
| a page of 100 | 0.5 | 1.1 | 2.0 | 8.4 |
| histogram (hourly, then daily from 7 days) | 14 | 46 | 17 | — |
| four facets (daily from 3 days) | 30 | — | 33 | — |
| grouped, top 50 | 48 | 125 | 303 | 282 |
| a page of one kind and level, with the kind `indexed` | 2 | — | — | 66 |

Storage: an uncompressed day is 4.2 GB, of which 1.3 GB is index;
compressed, 0.4 GB. Thirty days, two of them uncompressed, are 17 GB. The
counts: 92,000 hourly rows a day, 183,000 per day and unit, and 2.2 million
per hour and unit — about a fifth of the rows, 275 MB a day before
compression. Every count has chunks as long as the table's and is
compressed after `compress_after`, as the rows are, in one run ordered by
its bucket: a late row's refresh rewrites only the compressed batches that
hold its bucket. One day's bucket of the counts per day and unit is still
183,000 rows, past the cap TimescaleDB puts on rows one statement may
decompress (100,000) — and before 2.21 that cap stopped the refresh, every
time, in the background. Hence the floor: the migration refuses an older
TimescaleDB by name. The late-row costs above were measured before the
counts were compressed or kept per hour and unit: a late row now
decompresses and recounts a day of those too.
Loading a
10-million-row day takes about 100 s; compressing it, 25 s. Ingesting
through the governed view with foreign keys checked: 58,000 rows a second.

What is still slow:

- a window of an hour at one-minute bars reads the rows: 272 ms, 547 ms
  scoped;
- a page entirely inside compressed history (ten days ending twenty days
  ago) decompresses its chunk to sort it: 960 ms from the table, 2.2 s
  through a governed view;
- grouped over a month counts every unit × kind of every day: about
  1.5 million rows of the per-`top` counts, ~300 ms. Over a day, exact
  from the counts per hour and `top` and the rows at its edges: ~230 ms
  (measured without the governed view; the rows alone took 516 ms).

Before the indexes above, a page of a month took 5–9 s: the newest chunk
had only TimescaleDB's default `(time DESC)` index, and the compressed
chunks were ordered by id, so every page sorted ten million rows.
