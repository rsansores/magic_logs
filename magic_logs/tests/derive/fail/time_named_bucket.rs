use chrono::{DateTime, Utc};
use magic_logs::MagicLog;

#[derive(MagicLog)]
#[log(name = "events", schema = "logs")]
pub struct Event {
    #[log(id)]
    pub id: i64,
    #[log(time)]
    pub bucket: DateTime<Utc>,
}
fn main() {}
