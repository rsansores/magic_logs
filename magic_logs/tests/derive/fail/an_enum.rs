use chrono::{DateTime, Utc};
use magic_logs::MagicLog;

#[derive(MagicLog)]
#[log(name = "events", schema = "logs")]
pub enum Event {
    Ping { at: DateTime<Utc> },
}
fn main() {}
