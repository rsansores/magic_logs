use chrono::{DateTime, Utc};
use magic_logs::MagicLog;
use uuid::Uuid;

#[derive(MagicLog)]
#[log(name = "events", schema = "logs")]
pub struct Event {
    #[log(id)]
    pub id: i64,
    #[log(time)]
    pub at: DateTime<Utc>,
    #[log(tenant, facet)]
    pub tenant_id: Uuid,
}
fn main() {}
