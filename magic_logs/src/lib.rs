//! One PostgreSQL + TimescaleDB time-series table, and the queries a log
//! explorer asks of it.
//!
//! A row type says, with `#[derive(MagicLog)]`, which column is its time,
//! which its level, which its facets, its `top` and its scope.
//! [`migration`] prints the SQL that makes the table a compressed hypertable
//! counted by hour, by day and by day and `top`; [`page`], [`histogram`],
//! [`facets`] and [`grouped`] build the queries, each read from the cheapest
//! table that answers it. Authorization is the host's: every relation is read
//! through a view the host makes, in [`Spec::view_schema`].
//!
//! Counts only, PostgreSQL only, one table. No query language, no alerting.

mod migration;
mod query;
mod spec;

pub use magic_logs_derive::MagicLog;
pub use migration::{migration, Governed};
pub use query::{
    counts_tier, facets, grouped, histogram, page, per_top_tier, Bucket, Cursor, FacetCount,
    Filter, Group,
};
pub use spec::{Column, Field, MagicLog, Spec, Storage, Tier};

/// What a filter or a question was refused for.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("the window ends before it begins")]
    Window,
    #[error("the table has no field `{0}`")]
    UnknownField(String),
    #[error("the field `{0}` is filtered twice")]
    FieldTwice(String),
    #[error("the table searches no column")]
    NothingSearched,
    #[error("the table has no `top` to group by")]
    NoTop,
}
