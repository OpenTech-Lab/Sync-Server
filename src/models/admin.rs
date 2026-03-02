use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{admin_audit_logs, admin_settings};

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize)]
#[diesel(table_name = admin_audit_logs)]
pub struct AdminAuditLog {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub target: Option<String>,
    pub details: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = admin_audit_logs)]
pub struct NewAdminAuditLog {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub target: Option<String>,
    pub details: Value,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize, Deserialize)]
#[diesel(table_name = admin_settings)]
#[diesel(primary_key(key))]
pub struct AdminSetting {
    pub key: String,
    pub value: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = admin_settings)]
pub struct NewAdminSetting {
    pub key: String,
    pub value: String,
}
