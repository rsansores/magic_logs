//! Every attribute, and what the derive writes from them.

use chrono::{DateTime, Utc};
use magic_logs::{Column, MagicLog};
use uuid::Uuid;

#[derive(MagicLog)]
#[log(
    name = "events",
    schema = "logs",
    view_schema = "api",
    chunk = "6 hours",
    compress_after = "1 day",
    segment_by = "unit_id",
    search_with = "f_unaccent"
)]
pub struct Event {
    #[log(id)]
    pub id: Uuid,
    #[log(time)]
    pub at: DateTime<Utc>,
    #[log(tenant)]
    pub tenant_id: Uuid,
    #[log(scope)]
    pub zone_id: Uuid,
    #[log(level, sql = "severity")]
    pub severity: String,
    #[log(facet, per_top, indexed)]
    pub kind: String,
    #[log(top)]
    pub unit_id: Uuid,
    #[log(filter)]
    pub operator_id: Option<Uuid>,
    #[log(search)]
    pub description: String,
    #[log(search)]
    pub tag: Option<String>,
    pub not_a_log_column: Vec<u8>,
}

fn main() {
    let spec = Event::SPEC;
    assert_eq!((spec.name, spec.schema, spec.view_schema), ("events", "logs", "api"));
    assert_eq!(spec.time, "at");
    assert_eq!(spec.id, Column { name: "id", sql: "uuid" });
    assert_eq!(spec.tenant, Some("tenant_id"));
    assert_eq!(spec.scope, Some("zone_id"));
    assert_eq!(spec.level, Some(Column { name: "severity", sql: "severity" }));
    assert_eq!(spec.top, Some(Column { name: "unit_id", sql: "uuid" }));
    let fields: Vec<_> = spec
        .fields
        .iter()
        .map(|f| (f.column.name, f.counted, f.per_top, f.indexed))
        .collect();
    assert_eq!(
        fields,
        [
            ("tenant_id", true, true, false),
            ("zone_id", true, true, false),
            ("severity", true, true, false),
            ("kind", true, true, true),
            ("unit_id", false, false, false),
            ("operator_id", false, false, false),
        ]
    );
    assert_eq!(spec.field("operator_id").unwrap().column.sql, "uuid");
    assert_eq!(spec.search, ["description", "tag"]);
    assert_eq!(spec.search_with, Some("f_unaccent"));
    assert_eq!(
        (spec.storage.chunk, spec.storage.compress_after, spec.storage.segment_by),
        ("6 hours", "1 day", Some("unit_id"))
    );
}
