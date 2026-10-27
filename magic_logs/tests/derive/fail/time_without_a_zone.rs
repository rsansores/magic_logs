use chrono::{DateTime, NaiveDateTime, Utc};
use magic_logs::MagicLog;

#[derive(MagicLog)]
#[log(name = "events", schema = "logs")]
pub struct Event {
    #[log(id)]
    pub id: i64,
    #[log(time, sql = "timestamp")]
    pub at: NaiveDateTime,
    #[log(facet)]
    pub kind: Option<DateTime<Utc>>,
}
fn main() {}
