use chrono::{DateTime, NaiveDateTime, Utc};
use magic_logs::MagicLog;

#[derive(MagicLog)]
#[log(name = "events", schema = "logs")]
pub struct Event {
    #[log(id, time)]
    pub at: DateTime<Utc>,
}
fn main() {}
