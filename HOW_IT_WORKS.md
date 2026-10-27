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

Three continuous aggregates — the counts — read it:

| relation | bucket | kept by | read for |
|---|---|---|---|
| `<name>_hourly` | an hour | the tenant, the scope, the level, every `facet` | short windows |
| `<name>_daily` | a day, from the hourly | the same | windows past three days |
| `<name>_daily_<top>` | a day | the tenant, the `top`, the scope, the level, every `per_top` facet; first and last time | grouped, and the `top`'s facet |

The daily counts are a continuous aggregate over the hourly one, so a day
is summed from 24 rows, not from its events.

## The refresh

Every count is refreshed over all of history (`start_offset => NULL`): the
hourly and per-`top` ones every five minutes up to an hour ago, the daily
one every hour up to a day ago. TimescaleDB keeps a log of which buckets
changed since the last refresh, so a refresh with nothing new costs
milliseconds (about 30 ms on the first host's 300 million rows), and one
after a late row recounts only the buckets that row fell in, however old.
What has not been refreshed yet is read from the rows
(`materialized_only = false`): a count always includes the last hour.

What a late row costs, measured: 1,000 rows sixty days late, into a new
chunk, 30 ms; 1,000 rows into a compressed day twenty days back, 13 s, the
whole day recounted; a backlog of 105,000 rows spread over twenty-one
days, about six minutes. That runs in the background job, not in a reader's
request, and a fleet of devices that reconnect after weeks keeps the job
busy rather than the readers waiting. No window is cut off: a late row is
counted by the next refresh of each count — within five minutes for the
hourly and per-`top` counts, within the hour for the daily — and until then
the counts of its bucket are short of the rows by it. A row newer than the
last refresh is counted at once (real-time aggregation).

## Which question reads what

A question names a window, values of some fields, and maybe a search.
`counts_tier` and `per_top_tier` pick the smallest table that holds every
field it names:

- a **histogram** reads the daily counts for a step of whole days, the
  hourly ones for a step of whole hours; a step finer than an hour, a
  search, or a field the counts do not keep (a `filter`) reads the rows;
- a **facet** reads the daily counts for a window longer than three days,
  the hourly ones for a shorter one; the `top`'s facet reads the counts per
  day and `top`;
- **grouped** reads the counts per day and `top` when every field the
  filter names, and the grouped one, are kept there;
- a **page** always reads the rows.

A question that names the `top` (one unit) and asks by whole days reads
the counts per day and `top`, when every other field it names is kept
there (`per_top`): the hourly and daily counts do not keep the `top`.

A count is of whole buckets: the window `[from, to)` takes every bucket
that begins in `[time_bucket(from), to)`. A window from 10:20 counts from
10:00 in the hourly counts and from midnight UTC in the daily ones; the
rows answer to the second. An explorer that shows a count rounds it
(`1.2k`), and the rows it lists are always exact.

## Facets

A facet counts each value of one field under every other field's filter
but its own: with `kind = door` chosen, the `kind` facet still counts every
kind, so a reader sees what choosing a second would add. Each field is one
`SELECT … GROUP BY` with its own filter left out, each from its own tier,
and the fields are one statement (`UNION ALL`): one round trip.

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
counts: 92,000 hourly rows a day, 183,000 per day and unit. Loading a
10-million-row day takes about 100 s; compressing it, 25 s. Ingesting
through the governed view with foreign keys checked: 58,000 rows a second.

What is still slow:

- a window of an hour at one-minute bars reads the rows: 272 ms, 547 ms
  scoped;
- a page entirely inside compressed history (ten days ending twenty days
  ago) decompresses its chunk to sort it: 960 ms from the table, 2.2 s
  through a governed view;
- grouped over a month counts every unit × kind of every day: about
  1.5 million rows of the per-`top` counts, ~300 ms.

Before the indexes above, a page of a month took 5–9 s: the newest chunk
had only TimescaleDB's default `(time DESC)` index, and the compressed
chunks were ordered by id, so every page sorted ten million rows.
