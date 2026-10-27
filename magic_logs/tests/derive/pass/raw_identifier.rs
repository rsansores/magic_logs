//! A column named like a Rust keyword is the column of that name.

use chrono::{DateTime, Utc};
use magic_logs::MagicLog;

#[derive(MagicLog)]
#[log(name = "events", schema = "logs")]
pub struct Event {
    #[log(id)]
    pub id: i64,
    #[log(time)]
    pub at: DateTime<Utc>,
    #[log(facet, indexed)]
    pub r#type: String,
}

fn main() {
    assert_eq!(Event::SPEC.fields[0].column.name, "type");
    assert!(Event::SPEC.field("type").is_some());
}
