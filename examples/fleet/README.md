# The fleet

A delivery fleet's events in one TimescaleDB table: thirty days of 36
trucks in five zones, an incident in the harbor an hour and a half ago,
and a few events more every second. The server serves magic_logs' four
questions as JSON, in the shapes `magic_logs_explorer` reads.

```sh
docker compose up -d     # PostgreSQL 18 with TimescaleDB 2.31, on 5435
cargo run                # makes the table a log table, seeds it once, serves :3001
```

Then the explorer's demo, reading it:

```sh
cd ../../explorer && pnpm install && pnpm dev
```

and open http://localhost:5198/?server.

- `schema.sql`: the table as a host has it before magic_logs: in a schema
  of its own (`fleet`), its primary key holding the time.
- `src/main.rs`:
  - `Event`, the row, and what each column is to the explorer
    (`#[derive(MagicLog)]`);
  - `set_up`: the migration, once, in one transaction, each relation made
    readable through a view in `api` by `govern`; the seed, once;
  - `govern`: the fleet's authorization. A view keeps the zones the
    reader may see — every zone, unless the request's `x-zones` header
    names some — and every question reads through those views;
  - the handlers: `/api/page`, `/api/histogram`, `/api/facets`,
    `/api/grouped`, each in a transaction of its own that carries the
    reader's zones, a refused question answered 400.

```sh
curl -H 'x-zones: Harbor' "localhost:3001/api/facets?from=2026-10-01T00:00:00Z&to=2026-10-31T00:00:00Z"
```

`DATABASE_URL` points the server elsewhere, and `FLEET_LISTEN`
(`127.0.0.1:3001` by default) is where it listens. The example reaches the
crate in this repository by path; a copy outside it leaves `path` out of
`Cargo.toml`.
