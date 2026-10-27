//! A table of a time and an id: the defaults fill in the rest.

use chrono::{DateTime, Utc};
use magic_logs::MagicLog;

#[derive(MagicLog)]
#[log(name = "pings", schema = "logs")]
pub struct Ping {
    #[log(id)]
    pub id: i64,
    #[log(time)]
    pub at: DateTime<Utc>,
}

fn main() {
    let spec = Ping::SPEC;
    assert_eq!(spec.view_schema, "public");
    assert_eq!((spec.storage.chunk, spec.storage.compress_after), ("1 day", "2 days"));
    assert_eq!(spec.storage.segment_by, None);
    assert_eq!(spec.id.sql, "bigint");
    assert!(spec.fields.is_empty() && spec.search.is_empty());
    assert!(spec.level.is_none() && spec.top.is_none() && spec.tenant.is_none());
}
