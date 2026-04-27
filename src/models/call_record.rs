use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::call_records;

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize)]
#[diesel(table_name = call_records)]
pub struct CallRecord {
    pub id: Uuid,
    pub caller_id: Uuid,
    pub callee_id: Uuid,
    pub call_type: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = call_records)]
pub struct NewCallRecord {
    pub id: Uuid,
    pub caller_id: Uuid,
    pub callee_id: Uuid,
    pub call_type: String,
    pub status: String,
}
