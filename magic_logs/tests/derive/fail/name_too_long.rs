use chrono::{DateTime, Utc};
use magic_logs::MagicLog;

#[derive(MagicLog)]
#[log(name = "the_events_every_unit_of_the_fleet_has_ever_recorded", schema = "logs")]
pub struct Event {
    #[log(id)]
    pub id: i64,
    #[log(time)]
    pub at: DateTime<Utc>,
    #[log(facet, indexed)]
    pub kind: String,
}
fn main() {}
