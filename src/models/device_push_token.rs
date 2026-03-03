use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use uuid::Uuid;

use crate::schema::device_push_tokens;

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize)]
#[diesel(table_name = device_push_tokens)]
pub struct DevicePushToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub platform: String,
    pub token: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = device_push_tokens)]
pub struct NewDevicePushToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub platform: String,
    pub token: String,
    pub last_seen_at: Option<DateTime<Utc>>,
}
