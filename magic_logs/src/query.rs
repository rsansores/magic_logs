//! The explorer's four questions, each read from the cheapest tables that
//! answer it: the rows for a page, and the counts — by hour and by day, and
//! by hour and by day per `top` — unless the filter names something they do
//! not keep.

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
    /// them, and refuses one that does not read with a data exception
    /// (SQLSTATE class `22`: `22P02`, `22003`, `22007`, …). An empty list
    /// leaves the field unfiltered. The rows without a value (`NULL`), which
    /// a facet counts as `None`, are not selectable.
    pub fields: Vec<(String, Vec<String>)>,
    /// Words the searched columns contain, every one, in any order and in
    /// any of the columns: `%`, `_` and `\\` are themselves, not patterns.
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

    /// The rows whose searched columns contain every word of `text`, in any
    /// order. A blank `text` is no search: every row, a column without a
    /// value included.
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
        if self.searched().is_some() && spec.search.is_empty() {
            return Err(Error::NothingSearched);
        }
        Ok(())
    }

    /// The search, unless it is blank.
    fn searched(&self) -> Option<&str> {
        self.search.as_deref().filter(|t| !t.trim().is_empty())
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

/// One stretch of a window, and the table that counts it exactly: the
/// rows, or counts whose every bucket lies inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    /// The table read.
    pub tier: Tier,
    /// Where the stretch begins, inclusive.
    pub from: DateTime<Utc>,
    /// Where it ends, exclusive.
    pub to: DateTime<Utc>,
}

/// The counts that keep every field a question names: the counts by hour
/// and day, or those per `top`.
#[derive(Clone, Copy)]
enum Family {
    Counted,
    PerTop,
}

fn family(spec: &Spec, filter: &Filter, also: &[&str], per_top: bool) -> Option<Family> {
    if filter.searched().is_some() {
        return None;
    }
    let kept_in = |tier| {
        filter
            .named()
            .chain(also.iter().copied())
            .all(|name| keeps(spec, tier, name))
    };
    if !per_top && kept_in(Tier::Hourly) {
        Some(Family::Counted)
    } else if spec.top.is_some() && kept_in(Tier::HourlyTop) {
        Some(Family::PerTop)
    } else {
        None
    }
}

/// Whether `tier` keeps the column `name`.
fn keeps(spec: &Spec, tier: Tier, name: &str) -> bool {
    match tier {
        Tier::Raw => true,
        Tier::Hourly | Tier::Daily => spec.field(name).is_some_and(|f| f.counted),
        Tier::HourlyTop | Tier::DailyTop => {
            spec.top.is_some_and(|t| t.name == name) || spec.field(name).is_some_and(|f| f.per_top)
        }
    }
}

const HOUR: i64 = 3_600_000_000;
const DAY: i64 = 24 * HOUR;

fn floor(t: DateTime<Utc>, unit: i64) -> DateTime<Utc> {
    let micros = t.timestamp_micros();
    DateTime::from_timestamp_micros(micros - micros.rem_euclid(unit)).unwrap_or(t)
}

fn ceil(t: DateTime<Utc>, unit: i64) -> DateTime<Utc> {
    let down = floor(t, unit);
    if down == t {
        t
    } else {
        down + Duration::microseconds(unit)
    }
}

/// The table a histogram of `filter` in buckets of `step` reads: the daily
/// counts for a step of whole days, the hourly ones for whole hours — per
/// `top` when the filter names the `top` — and the rows for a finer step, a
/// search, or a field no count keeps. Each bar counts its bucket whole,
/// whichever it reads.
pub fn histogram_tier(spec: &Spec, filter: &Filter, step: Duration) -> Tier {
    let micros = step.num_microseconds().unwrap_or(i64::MAX);
    let whole = |unit: i64| micros >= unit && micros % unit == 0;
    let daily = whole(DAY);
    match family(spec, filter, &[], false) {
        _ if !whole(HOUR) => Tier::Raw,
        Some(Family::Counted) if daily => Tier::Daily,
        Some(Family::Counted) => Tier::Hourly,
        Some(Family::PerTop) if daily => Tier::DailyTop,
        Some(Family::PerTop) => Tier::HourlyTop,
        None => Tier::Raw,
    }
}

/// The stretches `filter`'s window is counted in, exactly, by a question
/// that also reads the columns `also` (a facet's field, a grouping's): the
/// whole days inside it from the daily counts, the whole hours beside them
/// from the hourly ones, and the minutes at either end from the rows. A
/// search, or a field no count keeps, reads the rows alone; so does a
/// window inside one hour.
pub fn parts(spec: &Spec, filter: &Filter, also: &[&str]) -> Vec<Part> {
    parts_of(spec, filter, also, false)
}

/// [`parts`], from the counts per `top` alone when `per_top` (a grouping's
/// first and last row are kept there only).
fn parts_of(spec: &Spec, filter: &Filter, also: &[&str], per_top: bool) -> Vec<Part> {
    let (from, to) = (filter.from, filter.to);
    let rows = vec![Part {
        tier: Tier::Raw,
        from,
        to,
    }];
    let Some(family) = family(spec, filter, also, per_top) else {
        return rows;
    };
    let (hourly, daily) = match family {
        Family::Counted => (Tier::Hourly, Tier::Daily),
        Family::PerTop => (Tier::HourlyTop, Tier::DailyTop),
    };
    let (h0, h1) = (ceil(from, HOUR), floor(to, HOUR));
    if h0 >= h1 {
        return rows;
    }
    let (d0, d1) = (ceil(from, DAY), floor(to, DAY));
    let mut out = Vec::new();
    let mut push = |tier, from, to| {
        if from < to {
            out.push(Part { tier, from, to });
        }
    };
    push(Tier::Raw, from, h0);
    if d0 < d1 {
        push(hourly, h0, d0);
        push(daily, d0, d1);
        push(hourly, d1, h1);
    } else {
        push(hourly, h0, h1);
    }
    push(Tier::Raw, h1, to);
    out
}

fn source(spec: &Spec, tier: Tier) -> String {
    qualified(spec.view_schema, &spec.relation(tier))
}

/// What a question takes of the time: a stretch, exactly, or every bucket
/// of a step the window touches, whole.
enum Bounds {
    Stretch(DateTime<Utc>, DateTime<Utc>),
    Buckets(Duration),
}

/// `WHERE` the time, every field's values and the search.
fn push_where(
    qb: &mut QueryBuilder<'static, Postgres>,
    spec: &Spec,
    filter: &Filter,
    tier: Tier,
    bounds: Bounds,
) {
    let at = match tier {
        Tier::Raw => ident(spec.time),
        _ => "bucket".to_owned(),
    };
    if let Bounds::Buckets(step) = bounds {
        // A histogram counts every bucket of `step` the window touches,
        // whole, whatever it reads: each bar is its bucket's count.
        qb.push(format!(" WHERE {at} >= time_bucket("));
        qb.push_bind(step_interval(step));
        qb.push(", ");
        qb.push_bind(filter.from);
        qb.push(format!(") AND {at} < time_bucket("));
        qb.push_bind(step_interval(step));
        qb.push(", ");
        qb.push_bind(filter.to);
        qb.push(" - INTERVAL '1 microsecond') + ");
        qb.push_bind(step_interval(step));
    } else if let Bounds::Stretch(from, to) = bounds {
        // A stretch of counts begins and ends on its buckets: exact.
        qb.push(format!(" WHERE {at} >= "));
        qb.push_bind(from);
        qb.push(format!(" AND {at} < "));
        qb.push_bind(to);
    }
    for (name, values) in &filter.fields {
        if values.is_empty() {
            continue;
        }
        let Some(field) = spec.field(name) else {
            continue;
        };
        // The values are cast once, in an InitPlan: a cast from text that is
        // not immutable (to a timestamp) is no constant to TimescaleDB's
        // filter on compressed chunks, which then refuses it.
        let sql = field.column.sql;
        qb.push(format!(" AND {} = ANY((SELECT ", ident(name)));
        qb.push_bind(values.clone());
        qb.push(format!("::{sql}[])::{sql}[])"));
    }
    if let (Some(text), Tier::Raw) = (filter.searched(), tier) {
        let wrap = |s: String| match spec.search_with {
            Some(f) => format!("{f}({s})"),
            None => s,
        };
        // Every word, in any order, each in any searched column.
        for word in text.split_whitespace() {
            qb.push(" AND (");
            for (i, column) in spec.search.iter().enumerate() {
                if i > 0 {
                    qb.push(" OR ");
                }
                qb.push(format!(
                    "{} ILIKE ",
                    wrap(format!("{}::text", ident(column)))
                ));
                // The word passes through the function first, then is
                // escaped: the function may make a `%` of what it was given
                // (unaccent folds `％` into one).
                qb.push(format!(
                    "'%' || replace(replace(replace({}(",
                    spec.search_with.unwrap_or("")
                ));
                qb.push_bind(word.to_owned());
                qb.push(r"), '\', '\\'), '%', '\%'), '_', '\_') || '%'");
            }
            qb.push(")");
        }
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
    push_where(
        &mut qb,
        spec,
        filter,
        Tier::Raw,
        Bounds::Stretch(filter.from, filter.to),
    );
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
    // A step is whole microseconds, as PostgreSQL's intervals are.
    if step < Duration::microseconds(1) || step.subsec_nanos() % 1_000 != 0 {
        return Err(Error::Step);
    }
    let tier = histogram_tier(spec, filter, step);
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
    push_where(&mut qb, spec, filter, tier, Bounds::Buckets(step));
    qb.push(" GROUP BY 1, 2 ORDER BY 1, 2");
    Ok(qb)
}

/// How many rows hold each value of every field in `fields`, each counted
/// under every other field's filter but its own, so a reader sees what a
/// second value would add: rows of [`FacetCount`]. Exact: each field is
/// counted over the [`parts`] of the window, from the counts where they
/// keep it and the rows where they do not.
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
        if i > 0 {
            qb.push(" UNION ALL ");
        }
        qb.push(format!(
            "(SELECT {} AS field, value, sum(n)::bigint AS count FROM (",
            literal(name)
        ));
        for (j, part) in parts(spec, &alone, &[name]).into_iter().enumerate() {
            if j > 0 {
                qb.push(" UNION ALL ");
            }
            qb.push(format!(
                "SELECT {}::text AS value, {} AS n FROM {}",
                ident(name),
                counted(part.tier),
                source(spec, part.tier)
            ));
            push_where(
                &mut qb,
                spec,
                &alone,
                part.tier,
                Bounds::Stretch(part.from, part.to),
            );
            qb.push(" GROUP BY 1");
        }
        qb.push(") AS parts GROUP BY 2)");
    }
    if fields.is_empty() {
        qb.push("SELECT NULL::text AS field, NULL::text AS value, 0::bigint AS count WHERE false");
    }
    Ok(qb)
}

/// The `limit` largest groups of `top` × `by` under `filter`, with each
/// group's first and last row: rows of [`Group`]. Exact: counted over the
/// [`parts`] of the window, from the counts per `top` where they keep
/// every field named, and the rows otherwise.
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
    if spec.field(by).is_none() {
        return Err(Error::UnknownField(by.to_owned()));
    }
    let mut qb = QueryBuilder::new(
        "SELECT top, value, sum(n)::bigint AS count, min(first) AS first_at, max(last) AS last_at FROM (",
    );
    for (j, part) in parts_of(spec, filter, &[top.name, by], true)
        .into_iter()
        .enumerate()
    {
        if j > 0 {
            qb.push(" UNION ALL ");
        }
        let (first, last) = match part.tier {
            Tier::Raw => (
                format!("min({})", ident(spec.time)),
                format!("max({})", ident(spec.time)),
            ),
            _ => ("min(first_at)".to_owned(), "max(last_at)".to_owned()),
        };
        qb.push(format!(
            "SELECT {}::text AS top, {}::text AS value, {} AS n, {first} AS first, {last} AS last FROM {}",
            ident(top.name),
            ident(by),
            counted(part.tier),
            source(spec, part.tier)
        ));
        push_where(
            &mut qb,
            spec,
            filter,
            part.tier,
            Bounds::Stretch(part.from, part.to),
        );
        qb.push(" GROUP BY 1, 2");
    }
    qb.push(") AS parts GROUP BY 1, 2 ORDER BY 3 DESC, 1, 2 LIMIT ");
    qb.push_bind(limit);
    Ok(qb)
}

/// How a stretch counts its rows: one by one, or by the counts' sums.
fn counted(tier: Tier) -> &'static str {
    match tier {
        Tier::Raw => "count(*)::bigint",
        _ => "sum(events)::bigint",
    }
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
        // A packaged copy of the crate has no explorer beside it.
        if !std::path::Path::new(path)
            .parent()
            .is_some_and(|p| p.exists())
        {
            return;
        }
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
