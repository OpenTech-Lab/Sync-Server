use chrono::{DateTime, NaiveDate, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{daily_action_counters, guild_score_events, user_guild_stats};

pub const DEFAULT_DAILY_COUNTER_RETENTION_DAYS: i32 = 45;
pub const DEFAULT_SCORE_EVENT_RETENTION_DAYS: i32 = 180;

fn default_daily_counter_retention_days() -> i32 {
    DEFAULT_DAILY_COUNTER_RETENTION_DAYS
}

fn default_score_event_retention_days() -> i32 {
    DEFAULT_SCORE_EVENT_RETENTION_DAYS
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = user_guild_stats)]
#[diesel(primary_key(user_id))]
pub struct UserGuildStats {
    pub user_id: Uuid,
    pub active_days: i32,
    pub contribution_score: i32,
    pub derived_level: i32,
    pub derived_rank: String,
    pub last_active_day: Option<NaiveDate>,
    pub last_human_activity_at: Option<DateTime<Utc>>,
    pub suspicious_activity_streak: i32,
    pub automation_review_state: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = user_guild_stats)]
pub struct NewUserGuildStats {
    pub user_id: Uuid,
    pub active_days: i32,
    pub contribution_score: i32,
    pub derived_level: i32,
    pub derived_rank: String,
    pub last_active_day: Option<NaiveDate>,
    pub last_human_activity_at: Option<DateTime<Utc>>,
    pub suspicious_activity_streak: i32,
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
#[diesel(table_name = guild_score_events)]
pub struct GuildScoreEvent {
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
#[diesel(table_name = guild_score_events)]
pub struct NewGuildScoreEvent {
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
    pub daily_friend_add_limit_multiplier_percent: Option<i32>,
    pub daily_attachment_send_limit_multiplier_percent: Option<i32>,
    pub overrides_level_limits: bool,
}

fn rank_engine_enabled_default() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuildEnforcementConfig {
    pub enabled: bool,
    pub outbound_messages_enabled: bool,
    pub friend_adds_enabled: bool,
    pub attachment_sends_enabled: bool,
    /// When false, rank perks (score multipliers, overrides_level_limits) are not
    /// applied to limit calculations.  Level gating remains active.  Defaults to
    /// true so the rank engine is on unless explicitly disabled via config.
    #[serde(default = "rank_engine_enabled_default")]
    pub rank_engine_enabled: bool,
}

impl Default for GuildEnforcementConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            outbound_messages_enabled: true,
            friend_adds_enabled: true,
            attachment_sends_enabled: true,
            rank_engine_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuildPolicyConfig {
    #[serde(default = "GuildEnforcementConfig::default")]
    pub enforcement: GuildEnforcementConfig,
    #[serde(default = "default_daily_counter_retention_days")]
    pub daily_counter_retention_days: i32,
    #[serde(default = "default_score_event_retention_days")]
    pub score_event_retention_days: i32,
    pub level_policies: Vec<LevelPolicy>,
    pub rank_policies: Vec<RankPolicy>,
    pub community_upvote_daily_cap: i32,
    pub safe_attachment_types: Vec<String>,
}

/// Kind of progression milestone that triggered a client notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MilestoneKind {
    LevelUp,
    RankUp,
    UnlockAttachmentType,
}

/// Payload the client uses to render a milestone toast, badge, or unlock animation.
/// Returned as `pending_milestone_notification` in `GuildSnapshot` when a server-side
/// progression event was detected on this request.  Null when no milestone is pending.
///
/// The client is responsible for dismissing / persisting the notification locally;
/// the server does not maintain per-user notification state.  It is safe to ignore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildMilestoneNotification {
    pub kind: MilestoneKind,
    /// Human-readable badge label, e.g. "Level 5" or "Rank D".
    pub badge_label: String,
    /// Localisation key for the unlock headline, e.g. "guild.milestone.level_up".
    pub headline_key: String,
    /// Localisation key for the unlock sub-copy, e.g. "guild.milestone.level_5_detail".
    pub detail_key: String,
    /// Optional specific thing unlocked, e.g. "application/pdf".
    pub unlocked_value: Option<String>,
    /// New level or rank value after the milestone.
    pub new_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildSnapshot {
    pub active_days: i32,
    pub level: u8,
    pub contribution_score: i32,
    pub rank: String,
    pub next_level_active_days: Option<i32>,
    pub level_progress_percent: u8,
    pub daily_outbound_messages_enforced: bool,
    pub daily_outbound_messages_limit: Option<i32>,
    pub daily_outbound_messages_sent: i32,
    pub daily_outbound_messages_remaining: Option<i32>,
    pub daily_attachment_sends_enforced: bool,
    pub daily_attachment_send_limit: Option<i32>,
    pub daily_attachment_sends_sent: i32,
    pub daily_attachment_sends_remaining: Option<i32>,
    pub allowed_attachment_types: Vec<String>,
    pub daily_friend_adds_enforced: bool,
    pub daily_friend_add_limit: Option<i32>,
    pub daily_friend_adds_sent: i32,
    pub daily_friend_adds_remaining: Option<i32>,
    /// Client-visible progression challenge state.
    /// One of: "none", "challenged", "frozen".
    /// Does NOT expose internal automation_review_state labels beyond what the user needs.
    pub challenge_state: String,
    /// Non-null when a level-up, rank-up, or attachment-type unlock was detected on this
    /// request.  The client is responsible for dismissing the notification locally; the
    /// server does not persist per-user notification state.  Safe to ignore.
    pub pending_milestone_notification: Option<GuildMilestoneNotification>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuildHistoryPruneResult {
    pub daily_counter_retention_days: i32,
    pub score_event_retention_days: i32,
    pub pruned_before_day: NaiveDate,
    pub pruned_before_timestamp: DateTime<Utc>,
    pub daily_action_counters_deleted: i64,
    pub guild_score_events_deleted: i64,
}
