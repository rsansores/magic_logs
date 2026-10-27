# Contributing

Issues and pull requests are welcome.

## The tests

The suite needs a PostgreSQL with TimescaleDB preloaded (2.21 or later,
under its community license) whose user may create databases — each test
gets one of its own, and creates the extension in it:

```sh
docker run -d -p 5432:5432 -e POSTGRES_PASSWORD=postgres timescale/timescaledb-ha:pg18.6-ts2.31.0
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres
cargo test --workspace --all-features
```

`tests/explorer.rs` runs the migration on a real database and asks every
question twice — from the counts and from the rows (a search every test row matches reads them) — and holds the answers equal: a tier that counts wrong
fails there.

Each run includes the derive's suite (`magic_logs/tests/derive.rs`), which
builds every case under `magic_logs/tests/derive/` as a host would: what it
accepts compiles, and each misuse fails with the message checked in beside
it. The messages are the compiler's words, so they are held to one release
of rustc, named in the test (`PINNED`); on any other the suite builds what
is accepted and says it left the messages alone. CI's `diagnostics` job
runs them on the pinned one. A change to a message is written anew with
that toolchain and `TRYBUILD=overwrite`, and read before it is committed. A
misuse the derive refuses has a case under `tests/derive/fail/`; a new
refusal comes with its case.

Some tests compare what the crate makes with a file checked in:

- the SQL it writes (`magic_logs/tests/golden/`): the migration, and every
  question at each table it can read. `MAGIC_LOGS_BLESS=1` writes them
  anew. A change there is a change every host runs: read it as SQL.
- the explorer's row types (`explorer/src/core/contract/rows.ts`, generated
  with ts-rs from `Bucket`, `FacetCount` and `Group`) and a real answer to
  one question (`explorer/src/core/contract/fixtures/answers.ts`, written by
  the integration test). `UPDATE_CONTRACT=1` writes both. The explorer's
  type-check reads the fixture against the types, and its tests read the
  fixture.

## The explorer

`explorer/` is the npm package `magic_logs_explorer`, with pnpm:

```sh
cd explorer
pnpm install
pnpm test && pnpm type-check && pnpm lint && pnpm format:check && pnpm knip
pnpm dev          # the demo, on :5198; ?server reads examples/fleet
pnpm screenshot   # the README's pictures, with a headless Chromium
```

The package draws nothing: a change that styles a part belongs in the
demo. A picture in `explorer/images/` is written by `pnpm screenshot`, not
by hand, so it is always the demo as it is.

## The README

Every Rust block of the README is code of `magic_logs/tests/readme.rs`,
line for line, and a test of that file holds the two together: change one,
change the other. `scripts/check-links.sh` checks every link the documents
and the rustdoc comments make into the repository, anchors included,
against the tree, with no network; it needs [lychee](https://lychee.cli.rs).

## The public API

What a host can name is listed in `magic_logs/public-api/all-features.txt`,
and CI fails when the crate's API is not that list. A change to the API is a
change to the list, made on purpose:

```sh
rustup toolchain install nightly-2026-10-10 --profile minimal
cargo install cargo-public-api --version 0.52.0 --locked
cargo +nightly-2026-10-10 public-api -p magic_logs --all-features -s > magic_logs/public-api/all-features.txt
```

## The example

`examples/fleet` is a workspace of its own, with a lockfile of its own: run
`cargo fmt` and `cargo clippy` with `--manifest-path`, as CI does.

## What CI checks

`.github/workflows/ci.yml`, on every pull request and every push to main:

- **lint** — `cargo fmt --check` of the workspace and the example; `cargo
  clippy -D warnings` of the crates with every feature and with none, and of
  the example; the docs with `-D warnings`.
- **dependencies** — `cargo deny check` (advisories, licenses, sources and
  unmaintained crates anywhere in the graph; `deny.toml`) of the workspace
  and the example; **unused** — `cargo machete` over both. A scheduled
  workflow (`advisories.yml`) runs the advisories check daily, on days
  nothing is pushed, and Dependabot proposes the new versions of the
  actions, the crates and the explorer's npm packages.
- **workflows** — zizmor over `.github/`: every action pinned to a commit,
  no token left behind by a checkout, no expression injected into a script.
- **test** — the suite against TimescaleDB 2.21 on PostgreSQL 15, the oldest
  it supports, and TimescaleDB 2.31 on PostgreSQL 18.
- **diagnostics** — the derive's refusals, message by message, on the
  pinned rustc.
- **public-api** — the API against `magic_logs/public-api/`.
- **msrv** — the crates build on Rust 1.85.
- **example** — the fleet builds against the crate as it is.
- **explorer** — its formatting, lint, types, unused code (knip), tests,
  and the packed tarball as npm and TypeScript read it.
- **links** — every link into the repository resolves.
- **package** — one version everywhere (`scripts/check-versions.sh`), and
  `cargo package` of both crates, each verified by building the tarball.

## A change to the SQL

A table made by one version is a table the next one reads. A change to
what the migration writes — a tier, an index, a policy — says in the
changelog what a host runs on a table made before it, and the golden files
show the difference. A change to a question's SQL alone needs nothing from
a host: the golden files show it, and the integration test holds every tier
to the rows.
