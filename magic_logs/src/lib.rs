//! One PostgreSQL + TimescaleDB time-series table, and the queries a log
//! explorer asks of it.
//!
//! A row type says, with `#[derive(MagicLog)]`, which column is its time,
//! which its level, which its facets, its `top` and its scope.
//! [`migration`] prints the SQL that makes the table a compressed hypertable
//! counted by hour and by day, and per `top`; [`page`], [`histogram`],
//! [`facets`] and [`grouped`] build the queries, each read from the cheapest
//! table that answers it. Authorization is the host's: every relation is read
//! through a view the host makes, in [`Spec::view_schema`].
//!
//! ```no_run
//! use chrono::{DateTime, Duration, Utc};
//! use magic_logs::{histogram, page, Bucket, Filter, Governed, MagicLog};
//! use uuid::Uuid;
//!
//! #[derive(MagicLog, sqlx::FromRow)]
//! #[log(name = "events", schema = "logs")]
//! pub struct Event {
//!     #[log(id)]    pub id: Uuid,
//!     #[log(time)]  pub at: DateTime<Utc>,
//!     #[log(level)] pub level: String,
//!     #[log(facet)] pub kind: String,
//!     #[log(top)]   pub unit_id: Uuid,
//!     #[log(search)] pub message: String,
//! }
//!
//! # async fn run(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
//! // Once, in a migration: the table exists; this makes it a log table, and
//! // each relation readable through a view (here, by everyone).
//! let sql = magic_logs::migration(&Event::SPEC, |g: &Governed| {
//!     format!(
//!         "CREATE VIEW {} AS SELECT * FROM {};",
//!         g.quoted_view(),
//!         g.quoted_relation()
//!     )
//! });
//! sqlx::raw_sql(&sql).execute(&pool).await?;
//!
//! // In a handler: a day of doors, by hour, and its newest rows.
//! let now = Utc::now();
//! let filter = Filter::new(now - Duration::days(1), now).with("kind", ["door"]);
//! let bars: Vec<Bucket> = histogram(&Event::SPEC, &filter, Duration::hours(1))?
//!     .build_query_as()
//!     .fetch_all(&pool)
//!     .await?;
//! let rows: Vec<Event> = page(&Event::SPEC, &filter, None, 50)?
//!     .build_query_as()
//!     .fetch_all(&pool)
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! [The README](https://github.com/rsansores/magic_logs) lists the
//! attributes; [how it works](https://github.com/rsansores/magic_logs/blob/main/HOW_IT_WORKS.md)
//! goes through the tables, the tiers and what each question costs.
//!
//! Counts only, PostgreSQL only, one table. No query language, no alerting.
//!
//! # Features
//!
//! - `serde`: `Serialize` on [`Bucket`], [`FacetCount`] and [`Group`], in
//!   the shapes the `magic_logs_explorer` npm package reads.
//!
//! # Public dependencies
//!
//! A new major version of any of these is a new major version of this crate:
//!
//! - [`sqlx`] 0.8: every question is a [`sqlx::QueryBuilder`], and the rows
//!   are [`sqlx::FromRow`].
//! - [`chrono`] 0.4: every instant is a [`chrono::DateTime<Utc>`](chrono::DateTime).

#![warn(missing_docs)]

mod migration;
mod query;
mod spec;

pub use magic_logs_derive::MagicLog;
pub use migration::{migration, Governed};
pub use query::{
    facets, grouped, histogram, histogram_tier, page, parts, Bucket, Cursor, FacetCount, Filter,
    Group, Part,
};
pub use spec::{Column, Field, MagicLog, Spec, Storage, Tier};

/// What a filter or a question was refused for.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The window ends where it begins, or before.
    #[error("the window ends before it begins")]
    Window,
    /// A filter, a facet or a grouping names a field the table does not have.
    #[error("the table has no field `{0}`")]
    UnknownField(String),
    /// A filter names the same field twice.
    #[error("the field `{0}` is filtered twice")]
    FieldTwice(String),
    /// A filter searches a table with no `#[log(search)]` column.
    #[error("the table searches no column")]
    NothingSearched,
    /// A histogram's step is shorter than a microsecond, or not whole
    /// microseconds.
    #[error("the step is not a whole number of microseconds, at least one")]
    Step,
    /// A page or a grouping asked for fewer than no rows.
    #[error("the limit is negative")]
    Limit,
    /// `grouped` asked of a table with no `#[log(top)]`.
    #[error("the table has no `top` to group by")]
    NoTop,
}
