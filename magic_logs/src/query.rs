//! The explorer's four questions, each read from the cheapest table that
//! answers it: the rows for a page, and the counts — by hour, by day, by day
//! and `top` — unless the filter names something they do not keep.

use chrono::{DateTime, Duration, Utc};
use sqlx::{Postgres, QueryBuilder};

use crate::migration::{ident, qualified};
use crate::spec::{Spec, Tier};
use crate::Error;

/// What an explorer is looking at: a window of time and the values each
/// named field may take (any of them; every field must match).
#[derive(Debug, Clone)]
pub struct Filter {
    /// Where the window begins, inclusive.
    pub from: DateTime<Utc>,
    /// Where it ends, exclusive.
    pub to: DateTime<Utc>,
    /// `(field, values)`: the row's value of `field` is one of `values`,
    /// written as SQL reads the field's type from text — the database casts
    /// them, and refuses one that does not read (`22P02`). An empty list
    /// leaves the field unfiltered. The rows without a value (`NULL`), which
    /// a facet counts as `None`, are not selectable.
    pub fields: Vec<(String, Vec<String>)>,
    /// Text the searched columns contain, as typed: `%`, `_` and `\\` are
    /// themselves, not patterns.
    pub search: Option<String>,
}

impl Filter {
    /// Every row of the window.
    pub fn new(from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        Self {
            from,
            to,
            fields: Vec::new(),
            search: None,
        }
    }

    /// The rows whose `field` is one of `values`; an empty `values` leaves
    /// the field unfiltered.
    pub fn with(
        mut self,
        field: &str,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.fields.push((
            field.to_owned(),
            values.into_iter().map(Into::into).collect(),
        ));
        self
    }

    /// The rows whose searched columns contain `text`.
    pub fn search(mut self, text: impl Into<String>) -> Self {
        self.search = Some(text.into());
        self
    }

    /// Refuses a window that ends before it begins, a field `spec` does not
    /// have, a field named twice and a search of a table that searches nothing.
    pub fn check(&self, spec: &Spec) -> Result<(), Error> {
        if self.to <= self.from {
            return Err(Error::Window);
        }
        for (i, (name, _)) in self.fields.iter().enumerate() {
            if spec.field(name).is_none() {
                return Err(Error::UnknownField(name.clone()));
            }
            if self.fields[..i].iter().any(|(n, _)| n == name) {
                return Err(Error::FieldTwice(name.clone()));
            }
        }
        if self.search.is_some() && spec.search.is_empty() {
            return Err(Error::NothingSearched);
        }
        Ok(())
    }

    fn named(&self) -> impl Iterator<Item = &str> {
        self.fields
            .iter()
            .filter(|(_, values)| !values.is_empty())
            .map(|(name, _)| name.as_str())
    }
}

/// Where a page ends: the next one begins after this row.
#[derive(Debug, Clone)]
pub struct Cursor {
    /// The row's time.
    pub time: DateTime<Utc>,
    /// The row's id, written as SQL reads the id's type from text.
    pub id: String,
}

/// One bar of the histogram: the rows of `level` in the bucket that begins
/// at `bucket`.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(test, derive(ts_rs::TS), ts(rename = "BucketRow"))]
pub struct Bucket {
    /// Where the bucket begins.
    pub bucket: DateTime<Utc>,
    /// The level, as text; `None` for rows without one, or a table with no level.
    pub level: Option<String>,
    /// How many rows.
    pub count: i64,
}

/// How many rows hold `value` in `field`, under every other field's filter.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(test, derive(ts_rs::TS), ts(rename = "FacetCountRow"))]
pub struct FacetCount {
    /// The field counted.
    pub field: String,
    /// The value, as text; `None` for the rows that hold none.
    pub value: Option<String>,
    /// How many rows hold it.
    pub count: i64,
}

/// One `top` and one value of the grouped field: how many rows, the first
/// and the last.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(test, derive(ts_rs::TS), ts(rename = "GroupRow"))]
pub struct Group {
    /// The `top`, as text.
    pub top: Option<String>,
    /// The grouped field's value, as text.
    pub value: Option<String>,
    /// How many rows.
    pub count: i64,
    /// The time of the group's first row.
    pub first_at: Option<DateTime<Utc>>,
    /// The time of its last.
    pub last_at: Option<DateTime<Utc>>,
}

/// Counts across more days than this are read per day.
const DAILY_FROM: i64 = 3;

/// The counts that answer `filter`, or the rows: in buckets of `step` for a
/// histogram, whole for a facet. Per day for a step of whole days, or a
/// facet over more than three days; per hour for a step of whole hours, or a
/// shorter facet; per day and `top` for a daily question that names the
/// `top`; the rows for a finer step, a search, or a field the counts do not
/// keep.
pub fn counts_tier(spec: &Spec, filter: &Filter, step: Option<Duration>) -> Tier {
    tier_keeping(spec, filter, step, None)
}

/// [`counts_tier`], of a tier that also keeps the column `also` (a facet's).
fn tier_keeping(spec: &Spec, filter: &Filter, step: Option<Duration>, also: Option<&str>) -> Tier {
    let whole = |unit: Duration| {
        let micros = |d: Duration| d.num_microseconds().unwrap_or(i64::MAX);
        step.is_none_or(|s| s >= unit && micros(s) % micros(unit) == 0)
    };
    if filter.search.is_some() || !whole(Duration::hours(1)) {
        return Tier::Raw;
    }
    let daily = match step {
        Some(_) => whole(Duration::days(1)),
        None => filter.to - filter.from > Duration::days(DAILY_FROM),
    };
    let kept_in = |tier| {
        filter
            .named()
            .chain(also)
            .all(|name| keeps(spec, tier, name))
    };
    if kept_in(Tier::Hourly) {
        if daily {
            Tier::Daily
        } else {
            Tier::Hourly
        }
    } else if daily && spec.top.is_some() && kept_in(Tier::DailyTop) {
        Tier::DailyTop
    } else {
        Tier::Raw
    }
}

/// Whether `tier` keeps the column `name`.
fn keeps(spec: &Spec, tier: Tier, name: &str) -> bool {
    match tier {
        Tier::Raw => true,
        Tier::Hourly | Tier::Daily => spec.field(name).is_some_and(|f| f.counted),
        Tier::DailyTop => {
            spec.top.is_some_and(|t| t.name == name) || spec.field(name).is_some_and(|f| f.per_top)
        }
    }
}

/// The counts per day and `top` that answer `filter` — a window longer than
/// three days whose every field they keep — or the rows.
pub fn per_top_tier(spec: &Spec, filter: &Filter) -> Tier {
    let kept = filter.named().all(|name| keeps(spec, Tier::DailyTop, name));
    // Whole days answer a window of days; a shorter one, its rows.
    let long = filter.to - filter.from > Duration::days(DAILY_FROM);
    if spec.top.is_none() || !kept || !long || filter.search.is_some() {
        Tier::Raw
    } else {
        Tier::DailyTop
    }
}

fn source(spec: &Spec, tier: Tier) -> String {
    qualified(spec.view_schema, &spec.relation(tier))
}

fn bucket_of(tier: Tier) -> &'static str {
    match tier {
        Tier::Hourly => "1 hour",
        _ => "1 day",
    }
}

/// `WHERE` the window, and every field but `except`.
fn push_where(
    qb: &mut QueryBuilder<'static, Postgres>,
    spec: &Spec,
    filter: &Filter,
    tier: Tier,
    except: Option<&str>,
) {
    if tier == Tier::Raw {
        let time = ident(spec.time);
        qb.push(format!(" WHERE {time} >= "));
        qb.push_bind(filter.from);
        qb.push(format!(" AND {time} < "));
        qb.push_bind(filter.to);
    } else {
        // A count's bucket is whole: the window takes the bucket it begins in.
        qb.push(format!(
            " WHERE bucket >= time_bucket(INTERVAL '{}', ",
            bucket_of(tier)
        ));
        qb.push_bind(filter.from);
        qb.push(") AND bucket < ");
        qb.push_bind(filter.to);
    }
    for (name, values) in &filter.fields {
        if values.is_empty() || Some(name.as_str()) == except {
            continue;
        }
        let Some(field) = spec.field(name) else {
            continue;
        };
        qb.push(format!(" AND {} = ANY(", ident(name)));
        qb.push_bind(values.clone());
        qb.push(format!("::{}[])", field.column.sql));
    }
    if let (Some(text), Tier::Raw) = (&filter.search, tier) {
        let wrap = |s: String| match spec.search_with {
            Some(f) => format!("{f}({s})"),
            None => s,
        };
        qb.push(" AND (");
        for (i, column) in spec.search.iter().enumerate() {
            if i > 0 {
                qb.push(" OR ");
            }
            qb.push(format!(
                "{} ILIKE ",
                wrap(format!("{}::text", ident(column)))
            ));
            // The text passes through the function first, then is escaped:
            // the function may make a `%` of what it was given (unaccent
            // folds `％` into one).
            qb.push(format!(
                "'%' || replace(replace(replace({}(",
                spec.search_with.unwrap_or("")
            ));
            qb.push_bind(text.clone());
            qb.push(r"), '\', '\\'), '%', '\%'), '_', '\_') || '%'");
        }
        qb.push(")");
    }
}

/// The next `limit` rows of `filter`, newest first, after `cursor`: every
/// column of the readable view, for the host to read as its row.
pub fn page(
    spec: &Spec,
    filter: &Filter,
    cursor: Option<&Cursor>,
    limit: i64,
) -> Result<QueryBuilder<'static, Postgres>, Error> {
    filter.check(spec)?;
    if limit < 0 {
        return Err(Error::Limit);
    }
    let (time, id) = (ident(spec.time), ident(spec.id.name));
    let mut qb = QueryBuilder::new(format!("SELECT * FROM {}", source(spec, Tier::Raw)));
    push_where(&mut qb, spec, filter, Tier::Raw, None);
    if let Some(cursor) = cursor {
        // The time alone bounds the chunks read: TimescaleDB excludes chunks,
        // and compressed segments, by a column's range, not by a row's.
        qb.push(format!(" AND {time} <= "));
        qb.push_bind(cursor.time);
        qb.push(format!(" AND ({time}, {id}) < ("));
        qb.push_bind(cursor.time);
        qb.push(", ");
        qb.push_bind(cursor.id.clone());
        qb.push(format!("::{})", spec.id.sql));
    }
    qb.push(format!(" ORDER BY {time} DESC, {id} DESC LIMIT "));
    qb.push_bind(limit);
    Ok(qb)
}

/// The histogram of `filter` in buckets of `step`, stacked by level: rows
/// of [`Bucket`].
pub fn histogram(
    spec: &Spec,
    filter: &Filter,
    step: Duration,
) -> Result<QueryBuilder<'static, Postgres>, Error> {
    filter.check(spec)?;
    if step <= Duration::zero() {
        return Err(Error::Step);
    }
    let tier = counts_tier(spec, filter, Some(step));
    let level = spec.level.map_or("NULL::text".to_owned(), |l| {
        format!("{}::text", ident(l.name))
    });
    let (at, count) = match tier {
        Tier::Raw => (ident(spec.time), "count(*)"),
        _ => ("bucket".to_owned(), "sum(events)::bigint"),
    };
    let mut qb = QueryBuilder::new("SELECT time_bucket(");
    qb.push_bind(step_interval(step));
    qb.push(format!(
        ", {at}) AS bucket, {level} AS level, {count} AS count FROM {}",
        source(spec, tier)
    ));
    push_where(&mut qb, spec, filter, tier, None);
    qb.push(" GROUP BY 1, 2 ORDER BY 1, 2");
    Ok(qb)
}

/// How many rows hold each value of every field in `fields`, each counted
/// under every other field's filter but its own, so a reader sees what a
/// second value would add: rows of [`FacetCount`]. The `top` is counted per
/// `top`; the rest by hour or by day.
pub fn facets(
    spec: &Spec,
    filter: &Filter,
    fields: &[&str],
) -> Result<QueryBuilder<'static, Postgres>, Error> {
    filter.check(spec)?;
    let top = spec.top.map(|t| t.name);
    let mut qb = QueryBuilder::new("");
    for (i, name) in fields.iter().enumerate() {
        if spec.field(name).is_none() && Some(*name) != top {
            return Err(Error::UnknownField((*name).to_owned()));
        }
        let mut alone = filter.clone();
        alone.fields.retain(|(n, _)| n != name);
        let tier = if Some(*name) == top {
            per_top_tier(spec, &alone)
        } else {
            tier_keeping(spec, &alone, None, Some(name))
        };
        let count = if tier == Tier::Raw {
            "count(*)"
        } else {
            "sum(events)::bigint"
        };
        if i > 0 {
            qb.push(" UNION ALL ");
        }
        qb.push(format!(
            "(SELECT {} AS field, {}::text AS value, {count} AS count FROM {}",
            literal(name),
            ident(name),
            source(spec, tier)
        ));
        push_where(&mut qb, spec, &alone, tier, None);
        qb.push(" GROUP BY 2)");
    }
    if fields.is_empty() {
        qb.push("SELECT NULL::text AS field, NULL::text AS value, 0::bigint AS count WHERE false");
    }
    Ok(qb)
}

/// The `limit` largest groups of `top` × `by` under `filter`, with each
/// group's first and last row: rows of [`Group`].
pub fn grouped(
    spec: &Spec,
    filter: &Filter,
    by: &str,
    limit: i64,
) -> Result<QueryBuilder<'static, Postgres>, Error> {
    filter.check(spec)?;
    if limit < 0 {
        return Err(Error::Limit);
    }
    let top = spec.top.ok_or(Error::NoTop)?;
    let field = spec
        .field(by)
        .ok_or_else(|| Error::UnknownField(by.to_owned()))?;
    let tier = match per_top_tier(spec, filter) {
        Tier::DailyTop if field.per_top => Tier::DailyTop,
        _ => Tier::Raw,
    };
    let (count, first, last) = match tier {
        Tier::Raw => (
            "count(*)".to_owned(),
            format!("min({})", ident(spec.time)),
            format!("max({})", ident(spec.time)),
        ),
        _ => (
            "sum(events)::bigint".to_owned(),
            "min(first_at)".to_owned(),
            "max(last_at)".to_owned(),
        ),
    };
    let mut qb = QueryBuilder::new(format!(
        "SELECT {}::text AS top, {}::text AS value, {count} AS count, {first} AS first_at, {last} AS last_at FROM {}",
        ident(top.name),
        ident(by),
        source(spec, tier)
    ));
    push_where(&mut qb, spec, filter, tier, None);
    qb.push(" GROUP BY 1, 2 ORDER BY 3 DESC, 1, 2 LIMIT ");
    qb.push_bind(limit);
    Ok(qb)
}

fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn step_interval(step: Duration) -> sqlx::postgres::types::PgInterval {
    sqlx::postgres::types::PgInterval {
        months: 0,
        days: 0,
        microseconds: step.num_microseconds().unwrap_or(i64::MAX),
    }
}

#[cfg(all(test, feature = "serde"))]
mod json {
    use super::*;
    use chrono::TimeZone;

    /// The names `magic_logs_explorer`'s `BucketRow`, `FacetCountRow` and
    /// `GroupRow` read.
    #[test]
    fn the_answers_read_as_the_explorer_types_them() {
        let at = Utc.with_ymd_and_hms(2026, 10, 1, 6, 0, 0).unwrap();
        let bucket = Bucket {
            bucket: at,
            level: Some("warning".into()),
            count: 3,
        };
        let facet = FacetCount {
            field: "kind".into(),
            value: None,
            count: 2,
        };
        let group = Group {
            top: Some("u1".into()),
            value: Some("door".into()),
            count: 5,
            first_at: Some(at),
            last_at: None,
        };
        assert_eq!(
            serde_json::to_value((bucket, facet, group)).unwrap(),
            serde_json::json!([
                { "bucket": "2026-10-01T06:00:00Z", "level": "warning", "count": 3 },
                { "field": "kind", "value": null, "count": 2 },
                { "top": "u1", "value": "door", "count": 5,
                  "first_at": "2026-10-01T06:00:00Z", "last_at": null }
            ])
        );
    }

    /// The explorer's types for the rows are these, generated: a field
    /// renamed, added or typed otherwise here changes them there, and the
    /// explorer's type-check reads its fixtures against them.
    /// `UPDATE_CONTRACT=1` writes them.
    #[test]
    fn the_explorers_types_are_generated_from_these() {
        use ts_rs::TS;
        let cfg = ts_rs::Config::new().with_large_int("number");
        let mut text = "// Generated from magic_logs' answer rows by its test \
                        `the_explorers_types_are_generated_from_these`: UPDATE_CONTRACT=1 rewrites it.\n"
            .to_owned();
        for (docs, decl) in [
            (Bucket::docs(), Bucket::decl(&cfg)),
            (FacetCount::docs(), FacetCount::decl(&cfg)),
            (Group::docs(), Group::decl(&cfg)),
        ] {
            text.push('\n');
            // Rust's intra-doc links read as plain code in TypeScript.
            text.push_str(
                &docs
                    .unwrap_or_default()
                    .replace("[`", "`")
                    .replace("`]", "`"),
            );
            text.push_str("export ");
            text.push_str(&decl);
            text.push('\n');
        }
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../explorer/src/core/contract/rows.ts"
        );
        if std::env::var_os("UPDATE_CONTRACT").is_some() {
            std::fs::write(path, &text).unwrap();
        }
        let written = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            written == text,
            "explorer/src/core/contract/rows.ts is not what the crate serializes: \
             UPDATE_CONTRACT=1 rewrites it"
        );
    }
}
