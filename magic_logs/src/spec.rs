//! What a log table is: its columns, and what each is to the explorer.

/// One time-series table, as the derive describes it.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    /// The table's name, and the name of the view it is read through.
    pub name: &'static str,
    /// The schema the table and its counts live in, which only the host's
    /// own work reads.
    pub schema: &'static str,
    /// The schema the views everyone else reads through live in.
    pub view_schema: &'static str,
    /// When the row happened: the hypertable's dimension and the order of a page.
    pub time: &'static str,
    /// The column that breaks ties between rows of the same instant.
    pub id: Column,
    /// The tenant column, kept in every count when there is one.
    pub tenant: Option<&'static str>,
    /// The column the governed views narrow a reader by, kept in every count.
    pub scope: Option<&'static str>,
    /// The ordered column the histogram is stacked by.
    pub level: Option<Column>,
    /// The entity counted on its own, per day: the grouped view's rows.
    pub top: Option<Column>,
    /// Every column a filter may name, in declaration order.
    pub fields: &'static [Field],
    /// Searched as text when a filter carries a search.
    pub search: &'static [&'static str],
    /// The function both sides of a search pass through (`f_unaccent`,
    /// `lower`); none compares with `ILIKE` alone.
    pub search_with: Option<&'static str>,
    /// How the table is stored.
    pub storage: Storage,
}

/// A column and its SQL type, which a filter's values are cast to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    /// The column's name, as the struct's field names it.
    pub name: &'static str,
    /// Its SQL type: what a filter's values, sent as text, are cast to.
    pub sql: &'static str,
}

/// A column a filter may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// The column.
    pub column: Column,
    /// Counted by hour and by day: filtering on it keeps the counts usable.
    pub counted: bool,
    /// Counted per `top` too: the grouped view may group by it.
    pub per_top: bool,
    /// Given an index of its own, `(field, time DESC, id DESC)`: a page
    /// narrowed to a rare value of it does not walk the whole range.
    pub indexed: bool,
}

/// Chunks, compression and the counts' refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Storage {
    /// One chunk's span: the newest chunk's indexes are what stays in memory.
    pub chunk: &'static str,
    /// How old a chunk is when it is compressed.
    pub compress_after: &'static str,
    /// The column a compressed chunk keeps its rows together by.
    pub segment_by: Option<&'static str>,
}

/// A table the counts are read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// The rows themselves.
    Raw,
    /// Counts per hour.
    Hourly,
    /// Counts per day, from the hourly ones.
    Daily,
    /// Counts per day and `top`, with the first and last row.
    DailyTop,
}

impl Spec {
    /// The field `name`, if a filter may name it.
    pub fn field(&self, name: &str) -> Option<&'static Field> {
        self.fields.iter().find(|f| f.column.name == name)
    }

    /// The relation `tier` is kept in, in [`Spec::schema`], unquoted.
    pub fn relation(&self, tier: Tier) -> String {
        match tier {
            Tier::Raw => self.name.to_owned(),
            Tier::Hourly => format!("{}_hourly", self.name),
            Tier::Daily => format!("{}_daily", self.name),
            Tier::DailyTop => format!(
                "{}_daily_{}",
                self.name,
                self.top
                    .map(|t| t.name.strip_suffix("_id").unwrap_or(t.name))
                    .unwrap_or("top")
            ),
        }
    }

    /// Every tier this table has: the counts per `top` only with a `top`.
    pub fn tiers(&self) -> Vec<Tier> {
        let mut tiers = vec![Tier::Raw, Tier::Hourly, Tier::Daily];
        if self.top.is_some() {
            tiers.push(Tier::DailyTop);
        }
        tiers
    }

    /// The columns the hourly and daily counts are kept by, in order.
    pub fn counted(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = self.tenant.into_iter().collect();
        for f in self.fields.iter().filter(|f| f.counted) {
            if !out.contains(&f.column.name) {
                out.push(f.column.name);
            }
        }
        out
    }

    /// The columns the counts per `top` are kept by, in order.
    pub fn counted_per_top(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = self.tenant.into_iter().collect();
        if let Some(top) = self.top {
            out.push(top.name);
        }
        for f in self.fields.iter().filter(|f| f.per_top) {
            if !out.contains(&f.column.name) {
                out.push(f.column.name);
            }
        }
        out
    }
}

/// A row type that is a log table: what `#[derive(MagicLog)]` implements.
pub trait MagicLog {
    /// The table, as the derive read it.
    const SPEC: Spec;
}
