use chrono::{DateTime, NaiveDate, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{daily_action_counters, trust_score_events, user_trust_stats};

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = user_trust_stats)]
#[diesel(primary_key(user_id))]
pub struct UserTrustStats {
    pub user_id: Uuid,
    pub active_days: i32,
    pub contribution_score: i32,
    pub derived_level: i32,
    pub derived_rank: String,
    pub last_active_day: Option<NaiveDate>,
    pub automation_review_state: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = user_trust_stats)]
pub struct NewUserTrustStats {
    pub user_id: Uuid,
    pub active_days: i32,
    pub contribution_score: i32,
    pub derived_level: i32,
    pub derived_rank: String,
    pub last_active_day: Option<NaiveDate>,
    pub automation_review_state: String,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = daily_action_counters)]
#[diesel(primary_key(user_id, action_key, day_bucket))]
pub struct DailyActionCounter {
    pub user_id: Uuid,
    pub action_key: String,
    pub day_bucket: NaiveDate,
    pub count: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = daily_action_counters)]
pub struct NewDailyActionCounter {
    pub user_id: Uuid,
    pub action_key: String,
    pub day_bucket: NaiveDate,
    pub count: i32,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize)]
#[diesel(table_name = trust_score_events)]
pub struct TrustScoreEvent {
    pub id: Uuid,
    pub user_id: Uuid,
    pub granter_user_id: Option<Uuid>,
    pub event_type: String,
    pub delta: i32,
    pub reference_id: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = trust_score_events)]
pub struct NewTrustScoreEvent {
    pub id: Uuid,
    pub user_id: Uuid,
    pub granter_user_id: Option<Uuid>,
    pub event_type: String,
    pub delta: i32,
    pub reference_id: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LevelPolicy {
    pub level: u8,
    pub min_active_days: i32,
    pub max_active_days: Option<i32>,
    pub daily_outbound_messages_limit: Option<i32>,
    pub daily_friend_add_limit: Option<i32>,
    pub daily_attachment_send_limit: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RankPolicy {
    pub rank: String,
    pub min_score: i32,
    pub max_score: Option<i32>,
    pub daily_outbound_messages_limit_multiplier_percent: Option<i32>,
    pub overrides_level_limits: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustPolicyConfig {
    pub level_policies: Vec<LevelPolicy>,
    pub rank_policies: Vec<RankPolicy>,
    pub community_upvote_daily_cap: i32,
    pub safe_attachment_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustSnapshot {
    pub active_days: i32,
    pub level: u8,
    pub contribution_score: i32,
    pub rank: String,
    pub next_level_active_days: Option<i32>,
    pub level_progress_percent: u8,
    pub daily_outbound_messages_limit: Option<i32>,
    pub daily_outbound_messages_sent: i32,
    pub daily_outbound_messages_remaining: Option<i32>,
}
