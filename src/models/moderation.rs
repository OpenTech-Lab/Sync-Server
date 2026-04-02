use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{moderation_reports, user_blocks};

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize)]
#[diesel(table_name = moderation_reports)]
pub struct ModerationReport {
    pub id: Uuid,
    pub reporter_user_id: Uuid,
    pub reported_user_id: Uuid,
    pub source: String,
    pub content_kind: String,
    pub content_id: Option<String>,
    pub reason_code: String,
    pub reporter_note: Option<String>,
    pub content_excerpt: Option<String>,
    pub status: String,
    pub resolution_action: Option<String>,
    pub resolution_notes: Option<String>,
    pub metadata: Value,
    pub review_due_at: DateTime<Utc>,
    pub reviewed_by_user_id: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = moderation_reports)]
pub struct NewModerationReport {
    pub id: Uuid,
    pub reporter_user_id: Uuid,
    pub reported_user_id: Uuid,
    pub source: String,
    pub content_kind: String,
    pub content_id: Option<String>,
    pub reason_code: String,
    pub reporter_note: Option<String>,
    pub content_excerpt: Option<String>,
    pub status: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize)]
#[diesel(table_name = user_blocks)]
#[diesel(primary_key(blocker_user_id, blocked_user_id))]
pub struct UserBlock {
    pub blocker_user_id: Uuid,
    pub blocked_user_id: Uuid,
    pub report_id: Option<Uuid>,
    pub reason_code: Option<String>,
    pub reporter_note: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = user_blocks)]
pub struct NewUserBlock {
    pub blocker_user_id: Uuid,
    pub blocked_user_id: Uuid,
    pub report_id: Option<Uuid>,
    pub reason_code: Option<String>,
    pub reporter_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSafetyStatePublic {
    pub current_terms_version: i32,
    pub accepted_terms_version: i32,
    pub terms_accepted_at: Option<DateTime<Utc>>,
    pub requires_terms_acceptance: bool,
}
