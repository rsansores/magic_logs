use chrono::{DateTime, Utc};
use magic_logs::MagicLog;

#[derive(MagicLog)]
#[log(name = "events", schema = "public")]
pub struct Event {
    #[log(id)]
    pub id: i64,
    #[log(time)]
    pub at: DateTime<Utc>,
}
fn main() {}
