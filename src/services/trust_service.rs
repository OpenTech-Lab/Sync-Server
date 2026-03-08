use chrono::{DateTime, Duration, NaiveDate, Utc};
use diesel::prelude::*;
use diesel::Connection;
use serde_json::Value;
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::admin::NewAdminAuditLog;
use crate::models::message::Message;
use crate::models::trust::{
    LevelPolicy, MilestoneKind, NewDailyActionCounter, NewTrustScoreEvent, NewUserTrustStats,
    RankPolicy, TrustEnforcementConfig, TrustHistoryPruneResult, TrustMilestoneNotification,
    TrustPolicyConfig, TrustScoreEvent, TrustSnapshot, UserTrustStats,
    DEFAULT_DAILY_COUNTER_RETENTION_DAYS, DEFAULT_SCORE_EVENT_RETENTION_DAYS,
};
use crate::schema::{
    admin_audit_logs, admin_settings, daily_action_counters, trust_score_events, user_trust_stats,
    users,
};
use crate::services::{admin_service, message_service};

const ACTION_OUTBOUND_MESSAGE: &str = "outbound_message";
const ACTION_ATTACHMENT_SEND: &str = "attachment_send";
const ACTION_FRIEND_ADD: &str = "friend_add";
const DEFAULT_AUTOMATION_REVIEW_STATE: &str = "clear";
const AUTOMATION_REVIEW_STATE_CHALLENGED: &str = "challenged";
const AUTOMATION_REVIEW_STATE_FROZEN: &str = "frozen";
const SUSPICIOUS_NEW_DAY_ACTIVITY_WINDOW_MINUTES: i64 = 10;
const SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD: i32 = 3;
const FROZEN_RECOVERY_WINDOW_HOURS: i64 = 24;
const DEFAULT_SAFE_ATTACHMENT_TYPES: &[&str] = &[
    "application/pdf",
    "image/gif",
    "image/jpeg",
    "image/png",
    "image/webp",
    "text/plain",
    "video/mp4",
];
const VALIDATED_MODERATION_ACTION_POINTS: i32 = 50;
const VERIFIED_ABUSE_REPORT_POINTS: i32 = 25;
const FALSE_REPORT_PENALTY_POINTS: i32 = -10;
const HELPFUL_ANSWER_VOTE_POINTS: i32 = 5;
pub const EVENT_VALIDATED_MODERATION_STICKER_REVIEW: &str =
    "validated_moderation_action.sticker_review";
pub const EVENT_VALIDATED_MODERATION_USER_SUSPEND: &str =
    "validated_moderation_action.user_suspend";
pub const EVENT_VERIFIED_ABUSE_REPORT: &str = "verified_abuse_report";
pub const EVENT_FALSE_REPORT_PENALTY: &str = "false_report_penalty";
pub const EVENT_HELPFUL_ANSWER_VOTE: &str = "helpful_answer_vote";

#[derive(Debug, Clone)]
pub enum SendMessageWithTrustResult {
    Sent {
        message: Message,
    },
    Limited {
        trust: TrustSnapshot,
        retry_after_seconds: i64,
    },
}

#[derive(Debug)]
pub enum AttachmentActionWithTrustResult<T> {
    Completed {
        value: T,
    },
    Limited {
        trust: TrustSnapshot,
        retry_after_seconds: i64,
    },
    UnsupportedMime {
        trust: TrustSnapshot,
    },
}

#[derive(Debug)]
pub enum ResolveContactWithTrustResult {
    Allowed,
    Limited {
        trust: TrustSnapshot,
        retry_after_seconds: i64,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ContributionEventOptions {
    pub requires_meaningful_granter: bool,
    pub daily_positive_cap: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContributionEventOutcome {
    pub event: Option<TrustScoreEvent>,
    pub contribution_score: i32,
    pub derived_rank: String,
    pub applied_delta: i32,
    pub duplicate: bool,
    pub suppressed_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HumanActivityAssessment {
    should_advance_active_day: bool,
    suspicious_activity_streak: i32,
    automation_review_state: &'static str,
}

pub fn read_trust_policy(pool: &Pool) -> Result<TrustPolicyConfig, AppError> {
    let mut conn = pool.get()?;
    load_trust_policy_conn(&mut conn)
}

pub fn save_trust_policy(
    pool: &Pool,
    policy: &TrustPolicyConfig,
) -> Result<TrustPolicyConfig, AppError> {
    let normalized = normalize_trust_policy(policy.clone())?;
    let encoded = serde_json::to_string(&normalized)
        .map_err(|error| AppError::Internal(anyhow::anyhow!("trust policy encode: {}", error)))?;
    admin_service::set_setting(pool, admin_service::SETTING_TRUST_POLICY, &encoded)?;
    Ok(normalized)
}

pub fn prune_trust_history(pool: &Pool) -> Result<TrustHistoryPruneResult, AppError> {
    let mut conn = pool.get()?;
    let policy = load_trust_policy_conn(&mut conn)?;
    let now = Utc::now();
    let pruned_before_day =
        now.date_naive() - Duration::days(i64::from(policy.daily_counter_retention_days));
    let pruned_before_timestamp =
        now - Duration::days(i64::from(policy.score_event_retention_days));

    let daily_action_counters_deleted = diesel::delete(
        daily_action_counters::table
            .filter(daily_action_counters::day_bucket.lt(pruned_before_day)),
    )
    .execute(&mut conn)? as i64;

    let trust_score_events_deleted = diesel::delete(
        trust_score_events::table
            .filter(trust_score_events::created_at.lt(pruned_before_timestamp)),
    )
    .execute(&mut conn)? as i64;

    Ok(TrustHistoryPruneResult {
        daily_counter_retention_days: policy.daily_counter_retention_days,
        score_event_retention_days: policy.score_event_retention_days,
        pruned_before_day,
        pruned_before_timestamp,
        daily_action_counters_deleted,
        trust_score_events_deleted,
    })
}

#[allow(dead_code)]
pub fn record_score_event(
    pool: &Pool,
    user_id: Uuid,
    granter_user_id: Option<Uuid>,
    event_type: &str,
    delta: i32,
    reference_id: Option<&str>,
    metadata: serde_json::Value,
) -> Result<TrustScoreEvent, AppError> {
    let mut conn = pool.get()?;
    diesel::insert_into(trust_score_events::table)
        .values(&NewTrustScoreEvent {
            id: Uuid::new_v4(),
            user_id,
            granter_user_id,
            event_type: event_type.trim().to_string(),
            delta,
            reference_id: reference_id.map(ToString::to_string),
            metadata,
        })
        .get_result::<TrustScoreEvent>(&mut conn)
        .map_err(AppError::from)
}

pub fn record_contribution_event(
    pool: &Pool,
    user_id: Uuid,
    granter_user_id: Option<Uuid>,
    event_type: &str,
    requested_delta: i32,
    reference_id: Option<&str>,
    metadata: Value,
    options: ContributionEventOptions,
) -> Result<ContributionEventOutcome, AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();
    let normalized_event_type = event_type.trim().to_string();
    let normalized_reference_id = reference_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    if normalized_event_type.is_empty() {
        return Err(AppError::BadRequest("event_type cannot be empty".into()));
    }

    conn.transaction(|conn| {
        let policy = load_trust_policy_conn(conn)?;
        let stats = ensure_user_trust_stats(conn, &policy, user_id)?;
        let stats = sync_derived_state(conn, &policy, stats, now)?;

        if let Some(reference_id_value) = normalized_reference_id.as_deref() {
            let existing = trust_score_events::table
                .filter(trust_score_events::user_id.eq(user_id))
                .filter(trust_score_events::event_type.eq(normalized_event_type.as_str()))
                .filter(trust_score_events::reference_id.eq(reference_id_value))
                .select(TrustScoreEvent::as_select())
                .first::<TrustScoreEvent>(conn)
                .optional()?;
            if existing.is_some() {
                return Ok(ContributionEventOutcome {
                    event: None,
                    contribution_score: stats.contribution_score,
                    derived_rank: stats.derived_rank,
                    applied_delta: 0,
                    duplicate: true,
                    suppressed_reason: Some("duplicate_reference".to_string()),
                });
            }
        }

        let mut applied_delta = requested_delta;
        let mut suppressed_reason = None;

        if requested_delta > 0 && options.requires_meaningful_granter {
            if granter_user_id == Some(user_id) {
                applied_delta = 0;
                suppressed_reason = Some("self_vote".to_string());
            } else if is_reciprocal_vote(conn, user_id, granter_user_id, &normalized_event_type, now)? {
                applied_delta = 0;
                suppressed_reason = Some("reciprocal_vote".to_string());
            } else if is_clustered_vote(conn, user_id, &normalized_event_type, now)? {
                applied_delta = 0;
                suppressed_reason = Some("clustered_vote".to_string());
            } else if !granter_is_meaningful(conn, &policy, granter_user_id, now)? {
                applied_delta = 0;
                suppressed_reason = Some("granter_not_eligible".to_string());
            }
        }

        if requested_delta > 0 {
            if let Some(daily_cap) = options.daily_positive_cap {
                let positive_points_today = positive_points_earned_today(
                    conn,
                    user_id,
                    normalized_event_type.as_str(),
                    now,
                )?;
                let remaining = (daily_cap - positive_points_today).max(0);
                if remaining == 0 {
                    applied_delta = 0;
                    suppressed_reason = Some("daily_cap_reached".to_string());
                } else if applied_delta > remaining {
                    applied_delta = remaining;
                    suppressed_reason = Some("daily_cap_truncated".to_string());
                }
            }
        }

        let next_contribution_score = (stats.contribution_score + applied_delta).max(0);
        let next_rank = rank_policy_for_score(&policy, next_contribution_score)
            .rank
            .clone();

        let event_metadata = enrich_contribution_event_metadata(
            metadata,
            requested_delta,
            applied_delta,
            suppressed_reason.as_deref(),
        );
        let event = diesel::insert_into(trust_score_events::table)
            .values(&NewTrustScoreEvent {
                id: Uuid::new_v4(),
                user_id,
                granter_user_id,
                event_type: normalized_event_type.clone(),
                delta: applied_delta,
                reference_id: normalized_reference_id.clone(),
                metadata: event_metadata,
            })
            .get_result::<TrustScoreEvent>(conn)?;

        diesel::update(user_trust_stats::table.find(user_id))
            .set((
                user_trust_stats::contribution_score.eq(next_contribution_score),
                user_trust_stats::derived_rank.eq(next_rank.as_str()),
                user_trust_stats::updated_at.eq(now),
            ))
            .execute(conn)?;

        Ok(ContributionEventOutcome {
            event: Some(event),
            contribution_score: next_contribution_score,
            derived_rank: next_rank,
            applied_delta,
            duplicate: false,
            suppressed_reason,
        })
    })
}

pub fn award_validated_moderation_action(
    pool: &Pool,
    actor_user_id: Uuid,
    event_type: &str,
    reference_id: Option<&str>,
    metadata: Value,
) -> Result<ContributionEventOutcome, AppError> {
    record_contribution_event(
        pool,
        actor_user_id,
        None,
        event_type,
        VALIDATED_MODERATION_ACTION_POINTS,
        reference_id,
        metadata,
        ContributionEventOptions::default(),
    )
}

pub fn award_verified_abuse_report(
    pool: &Pool,
    user_id: Uuid,
    reference_id: Option<&str>,
    metadata: Value,
) -> Result<ContributionEventOutcome, AppError> {
    record_contribution_event(
        pool,
        user_id,
        None,
        EVENT_VERIFIED_ABUSE_REPORT,
        VERIFIED_ABUSE_REPORT_POINTS,
        reference_id,
        metadata,
        ContributionEventOptions::default(),
    )
}

pub fn penalize_false_report(
    pool: &Pool,
    user_id: Uuid,
    reference_id: Option<&str>,
    metadata: Value,
) -> Result<ContributionEventOutcome, AppError> {
    record_contribution_event(
        pool,
        user_id,
        None,
        EVENT_FALSE_REPORT_PENALTY,
        FALSE_REPORT_PENALTY_POINTS,
        reference_id,
        metadata,
        ContributionEventOptions::default(),
    )
}

pub fn award_community_upvote(
    pool: &Pool,
    user_id: Uuid,
    granter_user_id: Option<Uuid>,
    reference_id: Option<&str>,
    metadata: Value,
    daily_cap: i32,
) -> Result<ContributionEventOutcome, AppError> {
    record_contribution_event(
        pool,
        user_id,
        granter_user_id,
        EVENT_HELPFUL_ANSWER_VOTE,
        HELPFUL_ANSWER_VOTE_POINTS,
        reference_id,
        metadata,
        ContributionEventOptions {
            requires_meaningful_granter: true,
            daily_positive_cap: Some(daily_cap),
        },
    )
}

/// Admin-issued score clawback.  The delta is expected to be negative (or zero = no-op).
/// Bypasses granter eligibility and daily cap checks since this is an authoritative admin action.
pub fn clawback_contribution_score(
    pool: &Pool,
    user_id: Uuid,
    delta: i32,
    reference_id: Option<&str>,
    metadata: Value,
) -> Result<ContributionEventOutcome, AppError> {
    let effective_delta = delta.min(0);
    record_contribution_event(
        pool,
        user_id,
        None,
        "admin.score_clawback",
        effective_delta,
        reference_id,
        metadata,
        ContributionEventOptions::default(),
    )
}

/// Admin-issued trust progression freeze.  Sets automation_review_state to "frozen" and logs
/// the action.  Does not change contribution score; use `clawback_contribution_score` if needed.
pub fn freeze_trust_progression(
    pool: &Pool,
    admin_user_id: Option<Uuid>,
    target_user_id: Uuid,
    reason: &str,
) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();
    diesel::update(user_trust_stats::table.find(target_user_id))
        .set((
            user_trust_stats::automation_review_state
                .eq(AUTOMATION_REVIEW_STATE_FROZEN),
            user_trust_stats::updated_at.eq(now),
        ))
        .execute(&mut conn)?;

    let details = serde_json::json!({
        "reason": reason,
        "target_user_id": target_user_id.to_string(),
        "admin_user_id": admin_user_id.map(|id| id.to_string()),
    });
    if let Some(admin_id) = admin_user_id {
        admin_service::append_audit_log(
            pool,
            Some(admin_id),
            "trust.admin.freeze_trust_progression",
            Some(&target_user_id.to_string()),
            details,
        )?;
    } else {
        insert_system_audit_log(
            &mut conn,
            "trust.system.freeze_trust_progression",
            Some(&target_user_id.to_string()),
            serde_json::json!({ "reason": reason }),
        )?;
    }
    Ok(())
}

/// Admin-issued trust progression unfreeze.  Resets automation_review_state to "clear".
pub fn unfreeze_trust_progression(
    pool: &Pool,
    admin_user_id: Uuid,
    target_user_id: Uuid,
    reason: &str,
) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();
    diesel::update(user_trust_stats::table.find(target_user_id))
        .set((
            user_trust_stats::automation_review_state
                .eq(DEFAULT_AUTOMATION_REVIEW_STATE),
            user_trust_stats::suspicious_activity_streak.eq(0),
            user_trust_stats::updated_at.eq(now),
        ))
        .execute(&mut conn)?;

    admin_service::append_audit_log(
        pool,
        Some(admin_user_id),
        "trust.admin.unfreeze_trust_progression",
        Some(&target_user_id.to_string()),
        serde_json::json!({ "reason": reason }),
    )
}

pub fn get_trust_snapshot(pool: &Pool, user_id: Uuid) -> Result<TrustSnapshot, AppError> {
    let mut conn = pool.get()?;
    let today = Utc::now().date_naive();
    let policy = load_trust_policy_conn(&mut conn)?;
    let stats = ensure_user_trust_stats(&mut conn, &policy, user_id)?;
    // Capture pre-sync level/rank to detect milestone transitions this request.
    let prev_level = stats.derived_level.clamp(1, 10) as u8;
    let prev_rank = stats.derived_rank.clone();
    let stats = sync_derived_state(&mut conn, &policy, stats, Utc::now())?;
    let sent_today = daily_action_count(&mut conn, user_id, ACTION_OUTBOUND_MESSAGE, today)?;
    let attachments_sent_today =
        daily_action_count(&mut conn, user_id, ACTION_ATTACHMENT_SEND, today)?;
    let friend_adds_today =
        daily_action_count(&mut conn, user_id, ACTION_FRIEND_ADD, today)?;

    Ok(build_snapshot(
        &policy,
        &stats,
        sent_today,
        attachments_sent_today,
        friend_adds_today,
        prev_level,
        &prev_rank,
    ))
}

pub fn record_human_activity(pool: &Pool, user_id: Uuid) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    let today = Utc::now().date_naive();
    let now = Utc::now();
    conn.transaction(|conn| {
        let policy = load_trust_policy_conn(conn)?;
        let stats = ensure_user_trust_stats(conn, &policy, user_id)?;
        let stats = sync_derived_state(conn, &policy, stats, now)?;
        let (stats, assessment) = update_human_activity_state(conn, stats, today, now)?;
        if assessment.should_advance_active_day {
            let _ = advance_active_day_if_needed(conn, &policy, stats, today, now)?;
        }
        Ok(())
    })
}

pub fn send_message_with_trust(
    pool: &Pool,
    sender_id: Uuid,
    recipient_id: Uuid,
    content: String,
) -> Result<SendMessageWithTrustResult, AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();
    let today = now.date_naive();

    conn.transaction(|conn| {
        let policy = load_trust_policy_conn(conn)?;
        let stats = ensure_user_trust_stats(conn, &policy, sender_id)?;
        let stats = sync_derived_state(conn, &policy, stats, now)?;
        let (stats, assessment) = update_human_activity_state(conn, stats, today, now)?;
        let stats = if assessment.should_advance_active_day {
            advance_active_day_if_needed(conn, &policy, stats, today, now)?
        } else {
            stats
        };
        let sent_today = daily_action_count(conn, sender_id, ACTION_OUTBOUND_MESSAGE, today)?;
        let attachments_sent_today =
            daily_action_count(conn, sender_id, ACTION_ATTACHMENT_SEND, today)?;
        let friend_adds_today =
            daily_action_count(conn, sender_id, ACTION_FRIEND_ADD, today)?;
        let level_policy = level_policy_for_active_days(&policy, stats.active_days);
        let rank_policy = rank_policy_for_limits(&policy, stats.contribution_score);
        let limit = effective_daily_outbound_messages_limit(level_policy, &rank_policy);
        let limit_enforced = outbound_message_limit_enforced(&policy);

        if limit_enforced {
            if let Some(limit) = limit {
                if sent_today >= limit {
                    let cur_level = stats.derived_level.clamp(1, 10) as u8;
                    let cur_rank = stats.derived_rank.clone();
                    return Ok(SendMessageWithTrustResult::Limited {
                        trust: build_snapshot(&policy, &stats, sent_today, attachments_sent_today, friend_adds_today, cur_level, &cur_rank),
                        retry_after_seconds: seconds_until_next_utc_day(now),
                    });
                }
            }
        }

        let message =
            message_service::insert_message_conn(conn, sender_id, recipient_id, &content)?;
        increment_daily_counter(conn, sender_id, ACTION_OUTBOUND_MESSAGE, today)?;

        Ok(SendMessageWithTrustResult::Sent { message })
    })
}

pub fn run_attachment_action_with_trust<T, F>(
    pool: &Pool,
    user_id: Uuid,
    mime_type: &str,
    operation: F,
) -> Result<AttachmentActionWithTrustResult<T>, AppError>
where
    F: FnOnce(&mut diesel::PgConnection) -> Result<T, AppError>,
{
    let mut conn = pool.get()?;
    let now = Utc::now();
    let today = now.date_naive();
    let normalized_mime_type = mime_type.trim().to_lowercase();

    conn.transaction(|conn| {
        let policy = load_trust_policy_conn(conn)?;
        let stats = ensure_user_trust_stats(conn, &policy, user_id)?;
        let stats = sync_derived_state(conn, &policy, stats, now)?;
        let (stats, assessment) = update_human_activity_state(conn, stats, today, now)?;
        let stats = if assessment.should_advance_active_day {
            advance_active_day_if_needed(conn, &policy, stats, today, now)?
        } else {
            stats
        };
        let outbound_messages_sent =
            daily_action_count(conn, user_id, ACTION_OUTBOUND_MESSAGE, today)?;
        let attachments_sent_today =
            daily_action_count(conn, user_id, ACTION_ATTACHMENT_SEND, today)?;
        let friend_adds_today =
            daily_action_count(conn, user_id, ACTION_FRIEND_ADD, today)?;

        if attachment_send_limit_enforced(&policy)
            && !attachment_type_allowed(&policy, &normalized_mime_type)
        {
            let cur_level = stats.derived_level.clamp(1, 10) as u8;
            let cur_rank = stats.derived_rank.clone();
            return Ok(AttachmentActionWithTrustResult::UnsupportedMime {
                trust: build_snapshot(
                    &policy,
                    &stats,
                    outbound_messages_sent,
                    attachments_sent_today,
                    friend_adds_today,
                    cur_level,
                    &cur_rank,
                ),
            });
        }

        let level_policy = level_policy_for_active_days(&policy, stats.active_days);
        let attachment_limit = effective_daily_attachment_send_limit(level_policy);
        if attachment_send_limit_enforced(&policy) {
            if let Some(limit) = attachment_limit {
                if attachments_sent_today >= limit {
                    let cur_level = stats.derived_level.clamp(1, 10) as u8;
                    let cur_rank = stats.derived_rank.clone();
                    return Ok(AttachmentActionWithTrustResult::Limited {
                        trust: build_snapshot(
                            &policy,
                            &stats,
                            outbound_messages_sent,
                            attachments_sent_today,
                            friend_adds_today,
                            cur_level,
                            &cur_rank,
                        ),
                        retry_after_seconds: seconds_until_next_utc_day(now),
                    });
                }
            }
        }

        let value = operation(conn)?;
        increment_daily_counter(conn, user_id, ACTION_ATTACHMENT_SEND, today)?;
        Ok(AttachmentActionWithTrustResult::Completed { value })
    })
}

pub fn resolve_contact_with_trust(
    pool: &Pool,
    user_id: Uuid,
) -> Result<ResolveContactWithTrustResult, AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();
    let today = now.date_naive();

    conn.transaction(|conn| {
        let policy = load_trust_policy_conn(conn)?;
        let stats = ensure_user_trust_stats(conn, &policy, user_id)?;
        let stats = sync_derived_state(conn, &policy, stats, now)?;
        let (stats, assessment) = update_human_activity_state(conn, stats, today, now)?;
        let stats = if assessment.should_advance_active_day {
            advance_active_day_if_needed(conn, &policy, stats, today, now)?
        } else {
            stats
        };

        let friend_adds_today = daily_action_count(conn, user_id, ACTION_FRIEND_ADD, today)?;
        let outbound_messages_sent =
            daily_action_count(conn, user_id, ACTION_OUTBOUND_MESSAGE, today)?;
        let attachments_sent_today =
            daily_action_count(conn, user_id, ACTION_ATTACHMENT_SEND, today)?;

        if friend_add_limit_enforced(&policy) {
            let level_policy = level_policy_for_active_days(&policy, stats.active_days);
            let limit = level_policy.daily_friend_add_limit;
            if let Some(limit) = limit {
                if friend_adds_today >= limit {
                    let cur_level = stats.derived_level.clamp(1, 10) as u8;
                    let cur_rank = stats.derived_rank.clone();
                    return Ok(ResolveContactWithTrustResult::Limited {
                        trust: build_snapshot(
                            &policy,
                            &stats,
                            outbound_messages_sent,
                            attachments_sent_today,
                            friend_adds_today,
                            cur_level,
                            &cur_rank,
                        ),
                        retry_after_seconds: seconds_until_next_utc_day(now),
                    });
                }
            }
        }

        increment_daily_counter(conn, user_id, ACTION_FRIEND_ADD, today)?;
        Ok(ResolveContactWithTrustResult::Allowed)
    })
}

fn outbound_message_limit_enforced(policy: &TrustPolicyConfig) -> bool {
    policy.enforcement.enabled && policy.enforcement.outbound_messages_enabled
}

fn attachment_send_limit_enforced(policy: &TrustPolicyConfig) -> bool {
    policy.enforcement.enabled && policy.enforcement.attachment_sends_enabled
}

fn friend_add_limit_enforced(policy: &TrustPolicyConfig) -> bool {
    policy.enforcement.enabled && policy.enforcement.friend_adds_enabled
}

fn attachment_type_allowed(policy: &TrustPolicyConfig, mime_type: &str) -> bool {
    policy
        .safe_attachment_types
        .iter()
        .any(|allowed| allowed == mime_type)
}

fn challenge_state_for_client(automation_review_state: &str) -> String {
    match automation_review_state {
        AUTOMATION_REVIEW_STATE_FROZEN => "frozen".to_string(),
        AUTOMATION_REVIEW_STATE_CHALLENGED => "challenged".to_string(),
        _ => "none".to_string(),
    }
}

fn build_snapshot(
    policy: &TrustPolicyConfig,
    stats: &UserTrustStats,
    daily_outbound_messages_sent: i32,
    daily_attachment_sends_sent: i32,
    daily_friend_adds_sent: i32,
    prev_level: u8,
    prev_rank: &str,
) -> TrustSnapshot {
    let level_policy = level_policy_for_active_days(policy, stats.active_days);
    let rank_policy = rank_policy_for_limits(policy, stats.contribution_score);
    let daily_outbound_messages_limit =
        effective_daily_outbound_messages_limit(level_policy, &rank_policy);
    let daily_outbound_messages_enforced = outbound_message_limit_enforced(policy);
    let daily_outbound_messages_remaining =
        daily_outbound_messages_limit.map(|limit| (limit - daily_outbound_messages_sent).max(0));
    let daily_attachment_send_limit = effective_daily_attachment_send_limit(level_policy);
    let daily_attachment_sends_enforced = attachment_send_limit_enforced(policy);
    let daily_attachment_sends_remaining =
        daily_attachment_send_limit.map(|limit| (limit - daily_attachment_sends_sent).max(0));
    let daily_friend_add_limit = level_policy.daily_friend_add_limit;
    let daily_friend_adds_enforced = friend_add_limit_enforced(policy);
    let daily_friend_adds_remaining =
        daily_friend_add_limit.map(|limit| (limit - daily_friend_adds_sent).max(0));

    let current_level = stats.derived_level.clamp(1, 10) as u8;
    let current_rank = &stats.derived_rank;

    let pending_milestone_notification = if current_level > prev_level {
        Some(TrustMilestoneNotification {
            kind: MilestoneKind::LevelUp,
            badge_label: format!("Level {current_level}"),
            headline_key: "trust.milestone.level_up".to_string(),
            detail_key: format!("trust.milestone.level_{current_level}_detail"),
            unlocked_value: None,
            new_value: current_level.to_string(),
        })
    } else if rank_order(current_rank) > rank_order(prev_rank) {
        Some(TrustMilestoneNotification {
            kind: MilestoneKind::RankUp,
            badge_label: format!("Rank {current_rank}"),
            headline_key: "trust.milestone.rank_up".to_string(),
            detail_key: format!("trust.milestone.rank_{}_detail", current_rank.to_lowercase()),
            unlocked_value: None,
            new_value: current_rank.clone(),
        })
    } else {
        None
    };

    TrustSnapshot {
        active_days: stats.active_days,
        level: current_level,
        contribution_score: stats.contribution_score,
        rank: current_rank.clone(),
        next_level_active_days: next_level_active_days(policy, level_policy.level),
        level_progress_percent: level_progress_percent(
            stats.active_days,
            level_policy.min_active_days,
            next_level_active_days(policy, level_policy.level),
        ),
        daily_outbound_messages_enforced,
        daily_outbound_messages_limit,
        daily_outbound_messages_sent,
        daily_outbound_messages_remaining,
        daily_attachment_sends_enforced,
        daily_attachment_send_limit,
        daily_attachment_sends_sent,
        daily_attachment_sends_remaining,
        allowed_attachment_types: policy.safe_attachment_types.clone(),
        daily_friend_adds_enforced,
        daily_friend_add_limit,
        daily_friend_adds_sent,
        daily_friend_adds_remaining,
        challenge_state: challenge_state_for_client(&stats.automation_review_state),
        pending_milestone_notification,
    }
}

fn enrich_contribution_event_metadata(
    metadata: Value,
    requested_delta: i32,
    applied_delta: i32,
    suppressed_reason: Option<&str>,
) -> Value {
    let mut object = match metadata {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("input".to_string(), other);
            map
        }
    };
    object.insert(
        "requested_delta".to_string(),
        serde_json::json!(requested_delta),
    );
    object.insert(
        "applied_delta".to_string(),
        serde_json::json!(applied_delta),
    );
    if let Some(reason) = suppressed_reason {
        object.insert("suppressed_reason".to_string(), serde_json::json!(reason));
    }
    Value::Object(object)
}

fn insert_system_audit_log(
    conn: &mut diesel::PgConnection,
    action: &str,
    target: Option<&str>,
    details: Value,
) -> Result<(), AppError> {
    diesel::insert_into(admin_audit_logs::table)
        .values(&NewAdminAuditLog {
            id: Uuid::new_v4(),
            actor_user_id: None,
            action: action.to_string(),
            target: target.map(ToString::to_string),
            details,
        })
        .execute(conn)?;
    Ok(())
}

fn ensure_user_trust_stats(
    conn: &mut diesel::PgConnection,
    policy: &TrustPolicyConfig,
    user_id: Uuid,
) -> Result<UserTrustStats, AppError> {
    let level_policy = level_policy_for_active_days(policy, 0);
    let rank_policy = rank_policy_for_score(policy, 0);
    diesel::insert_into(user_trust_stats::table)
        .values(&NewUserTrustStats {
            user_id,
            active_days: 0,
            contribution_score: 0,
            derived_level: i32::from(level_policy.level),
            derived_rank: rank_policy.rank.clone(),
            last_active_day: None,
            last_human_activity_at: None,
            suspicious_activity_streak: 0,
            automation_review_state: DEFAULT_AUTOMATION_REVIEW_STATE.to_string(),
        })
        .on_conflict(user_trust_stats::user_id)
        .do_nothing()
        .execute(conn)?;

    user_trust_stats::table
        .find(user_id)
        .select(UserTrustStats::as_select())
        .first::<UserTrustStats>(conn)
        .map_err(AppError::from)
}

fn sync_derived_state(
    conn: &mut diesel::PgConnection,
    policy: &TrustPolicyConfig,
    stats: UserTrustStats,
    now: DateTime<Utc>,
) -> Result<UserTrustStats, AppError> {
    let level_policy = level_policy_for_active_days(policy, stats.active_days);
    let rank_policy = rank_policy_for_score(policy, stats.contribution_score);

    if stats.derived_level == i32::from(level_policy.level)
        && stats.derived_rank == rank_policy.rank
    {
        return Ok(stats);
    }

    diesel::update(user_trust_stats::table.find(stats.user_id))
        .set((
            user_trust_stats::derived_level.eq(i32::from(level_policy.level)),
            user_trust_stats::derived_rank.eq(rank_policy.rank.as_str()),
            user_trust_stats::updated_at.eq(now),
        ))
        .get_result::<UserTrustStats>(conn)
        .map_err(AppError::from)
}

fn update_human_activity_state(
    conn: &mut diesel::PgConnection,
    stats: UserTrustStats,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Result<(UserTrustStats, HumanActivityAssessment), AppError> {
    let assessment = assess_human_activity(&stats, today, now);
    let updated = diesel::update(user_trust_stats::table.find(stats.user_id))
        .set((
            user_trust_stats::last_human_activity_at.eq(Some(now)),
            user_trust_stats::suspicious_activity_streak.eq(assessment.suspicious_activity_streak),
            user_trust_stats::automation_review_state.eq(assessment.automation_review_state),
            user_trust_stats::updated_at.eq(now),
        ))
        .get_result::<UserTrustStats>(conn)
        .map_err(AppError::from)?;
    if stats.automation_review_state != updated.automation_review_state {
        insert_system_audit_log(
            conn,
            "trust.review_state.changed",
            Some(&stats.user_id.to_string()),
            serde_json::json!({
                "previous_state": stats.automation_review_state,
                "new_state": updated.automation_review_state,
                "suspicious_activity_streak": updated.suspicious_activity_streak,
                "active_days": updated.active_days,
            }),
        )?;
    }
    Ok((updated, assessment))
}

fn assess_human_activity(
    stats: &UserTrustStats,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> HumanActivityAssessment {
    if stats.automation_review_state == AUTOMATION_REVIEW_STATE_FROZEN {
        let can_recover_from_frozen = stats
            .last_human_activity_at
            .map(|last_human_activity_at| {
                now.signed_duration_since(last_human_activity_at)
                    >= Duration::hours(FROZEN_RECOVERY_WINDOW_HOURS)
            })
            .unwrap_or(true)
            && stats.last_active_day != Some(today);

        if can_recover_from_frozen {
            return HumanActivityAssessment {
                should_advance_active_day: true,
                suspicious_activity_streak: 0,
                automation_review_state: DEFAULT_AUTOMATION_REVIEW_STATE,
            };
        }

        return HumanActivityAssessment {
            should_advance_active_day: false,
            suspicious_activity_streak: stats.suspicious_activity_streak,
            automation_review_state: AUTOMATION_REVIEW_STATE_FROZEN,
        };
    }

    let attempting_new_day = stats.last_active_day != Some(today);
    let suspicious_new_day_attempt = attempting_new_day
        && stats
            .last_human_activity_at
            .map(|last_human_activity_at| {
                now.signed_duration_since(last_human_activity_at)
                    < Duration::minutes(SUSPICIOUS_NEW_DAY_ACTIVITY_WINDOW_MINUTES)
            })
            .unwrap_or(false);

    let suspicious_activity_streak = if suspicious_new_day_attempt {
        stats.suspicious_activity_streak.saturating_add(1)
    } else if attempting_new_day {
        stats.suspicious_activity_streak.saturating_sub(1)
    } else {
        stats.suspicious_activity_streak
    };

    let automation_review_state =
        if suspicious_activity_streak >= SUSPICIOUS_ACTIVITY_FREEZE_THRESHOLD {
            AUTOMATION_REVIEW_STATE_FROZEN
        } else if suspicious_activity_streak > 0 {
            AUTOMATION_REVIEW_STATE_CHALLENGED
        } else {
            DEFAULT_AUTOMATION_REVIEW_STATE
        };

    HumanActivityAssessment {
        should_advance_active_day: attempting_new_day && !suspicious_new_day_attempt,
        suspicious_activity_streak,
        automation_review_state,
    }
}

fn advance_active_day_if_needed(
    conn: &mut diesel::PgConnection,
    policy: &TrustPolicyConfig,
    stats: UserTrustStats,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Result<UserTrustStats, AppError> {
    if stats.last_active_day == Some(today) {
        return Ok(stats);
    }

    let next_active_days = stats.active_days + 1;
    let level_policy = level_policy_for_active_days(policy, next_active_days);
    let rank_policy = rank_policy_for_score(policy, stats.contribution_score);
    diesel::update(user_trust_stats::table.find(stats.user_id))
        .set((
            user_trust_stats::active_days.eq(next_active_days),
            user_trust_stats::derived_level.eq(i32::from(level_policy.level)),
            user_trust_stats::derived_rank.eq(rank_policy.rank.as_str()),
            user_trust_stats::last_active_day.eq(Some(today)),
            user_trust_stats::updated_at.eq(now),
        ))
        .get_result::<UserTrustStats>(conn)
        .map_err(AppError::from)
}

fn daily_action_count(
    conn: &mut diesel::PgConnection,
    user_id: Uuid,
    action_key_value: &str,
    today: NaiveDate,
) -> Result<i32, AppError> {
    daily_action_counters::table
        .filter(daily_action_counters::user_id.eq(user_id))
        .filter(daily_action_counters::action_key.eq(action_key_value))
        .filter(daily_action_counters::day_bucket.eq(today))
        .select(daily_action_counters::count)
        .first::<i32>(conn)
        .optional()
        .map(|value| value.unwrap_or(0))
        .map_err(AppError::from)
}

fn increment_daily_counter(
    conn: &mut diesel::PgConnection,
    user_id: Uuid,
    action_key_value: &str,
    today: NaiveDate,
) -> Result<(), AppError> {
    diesel::insert_into(daily_action_counters::table)
        .values(&NewDailyActionCounter {
            user_id,
            action_key: action_key_value.to_string(),
            day_bucket: today,
            count: 1,
        })
        .on_conflict((
            daily_action_counters::user_id,
            daily_action_counters::action_key,
            daily_action_counters::day_bucket,
        ))
        .do_update()
        .set((
            daily_action_counters::count.eq(daily_action_counters::count + 1),
            daily_action_counters::updated_at.eq(Utc::now()),
        ))
        .execute(conn)?;
    Ok(())
}

fn granter_is_meaningful(
    conn: &mut diesel::PgConnection,
    policy: &TrustPolicyConfig,
    granter_user_id: Option<Uuid>,
    now: DateTime<Utc>,
) -> Result<bool, AppError> {
    let Some(granter_user_id) = granter_user_id else {
        return Ok(false);
    };

    let granter_role = users::table
        .find(granter_user_id)
        .select(users::role)
        .first::<String>(conn)
        .optional()?;

    let Some(granter_role) = granter_role else {
        return Ok(false);
    };

    if granter_role != "user" {
        return Ok(true);
    }

    let granter_stats = ensure_user_trust_stats(conn, policy, granter_user_id)?;
    let granter_stats = sync_derived_state(conn, policy, granter_stats, now)?;
    Ok(granter_stats.derived_level >= 4 || rank_at_least(&granter_stats.derived_rank, "E"))
}

/// Returns true if there is a recent upvote event of the same type in the reverse direction:
/// i.e. the current `recipient` (user_id) previously granted points to `granter_user_id`.
/// Checked within the last 30 days to catch ring abuse without penalising organic reciprocity
/// after a long gap.
fn is_reciprocal_vote(
    conn: &mut diesel::PgConnection,
    user_id: Uuid,
    granter_user_id: Option<Uuid>,
    event_type: &str,
    now: DateTime<Utc>,
) -> Result<bool, AppError> {
    let Some(granter_id) = granter_user_id else {
        return Ok(false);
    };
    let window_start = now - Duration::days(30);
    let exists = trust_score_events::table
        .filter(trust_score_events::user_id.eq(granter_id))
        .filter(trust_score_events::granter_user_id.eq(user_id))
        .filter(trust_score_events::event_type.eq(event_type))
        .filter(trust_score_events::delta.gt(0))
        .filter(trust_score_events::created_at.ge(window_start))
        .select(trust_score_events::id)
        .first::<Uuid>(conn)
        .optional()?;
    Ok(exists.is_some())
}

/// Returns true when the number of distinct grantors who awarded positive points to `user_id`
/// for the given event type on the current UTC day exceeds `CLUSTERED_VOTE_GRANTER_DAILY_THRESHOLD`.
/// This catches coordinated bot-farm pile-ons where many low-trust accounts vote for the same
/// target in a single day.
fn is_clustered_vote(
    conn: &mut diesel::PgConnection,
    user_id: Uuid,
    event_type: &str,
    now: DateTime<Utc>,
) -> Result<bool, AppError> {
    const CLUSTERED_VOTE_GRANTER_DAILY_THRESHOLD: i64 = 10;
    let day_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("valid UTC midnight");
    let day_start = DateTime::<Utc>::from_naive_utc_and_offset(day_start, Utc);

    // Count distinct non-null grantors who voted positively for this user today.
    let distinct_grantors: i64 = trust_score_events::table
        .filter(trust_score_events::user_id.eq(user_id))
        .filter(trust_score_events::event_type.eq(event_type))
        .filter(trust_score_events::created_at.ge(day_start))
        .filter(trust_score_events::delta.gt(0))
        .filter(trust_score_events::granter_user_id.is_not_null())
        .select(diesel::dsl::count_distinct(
            trust_score_events::granter_user_id,
        ))
        .first(conn)?;

    Ok(distinct_grantors >= CLUSTERED_VOTE_GRANTER_DAILY_THRESHOLD)
}

fn positive_points_earned_today(
    conn: &mut diesel::PgConnection,
    user_id: Uuid,
    event_type: &str,
    now: DateTime<Utc>,
) -> Result<i32, AppError> {
    let day_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("valid UTC midnight");
    let day_start = DateTime::<Utc>::from_naive_utc_and_offset(day_start, Utc);

    let total: Option<i64> = trust_score_events::table
        .filter(trust_score_events::user_id.eq(user_id))
        .filter(trust_score_events::event_type.eq(event_type))
        .filter(trust_score_events::created_at.ge(day_start))
        .filter(trust_score_events::delta.gt(0))
        .select(diesel::dsl::sum(trust_score_events::delta))
        .first(conn)?;

    Ok(total.unwrap_or(0).clamp(0, i64::from(i32::MAX)) as i32)
}

fn level_policy_for_active_days(policy: &TrustPolicyConfig, active_days: i32) -> &LevelPolicy {
    policy
        .level_policies
        .iter()
        .find(|entry| {
            active_days >= entry.min_active_days
                && entry
                    .max_active_days
                    .map(|max_active_days| active_days <= max_active_days)
                    .unwrap_or(true)
        })
        .unwrap_or_else(|| {
            policy
                .level_policies
                .last()
                .expect("trust policy must contain level policies")
        })
}

fn next_level_active_days(policy: &TrustPolicyConfig, level: u8) -> Option<i32> {
    policy
        .level_policies
        .iter()
        .find(|entry| entry.level > level)
        .map(|entry| entry.min_active_days)
}

fn rank_policy_for_score(policy: &TrustPolicyConfig, contribution_score: i32) -> &RankPolicy {
    policy
        .rank_policies
        .iter()
        .find(|entry| {
            contribution_score >= entry.min_score
                && entry
                    .max_score
                    .map(|max_score| contribution_score <= max_score)
                    .unwrap_or(true)
        })
        .unwrap_or_else(|| {
            policy
                .rank_policies
                .last()
                .expect("trust policy must contain rank policies")
        })
}

/// Returns the rank policy to use for **limit calculations**.
///
/// When `enforcement.rank_engine_enabled` is `false`, returns a neutral policy
/// with no multiplier and no level overrides so that level-based caps are
/// applied as if the user were rank F.  Rank display in the snapshot still
/// reflects the user's actual earned rank (stored in `derived_rank`).
fn rank_policy_for_limits(
    policy: &TrustPolicyConfig,
    contribution_score: i32,
) -> std::borrow::Cow<'_, RankPolicy> {
    if policy.enforcement.rank_engine_enabled {
        std::borrow::Cow::Borrowed(rank_policy_for_score(policy, contribution_score))
    } else {
        std::borrow::Cow::Owned(RankPolicy {
            rank: String::new(),
            min_score: 0,
            max_score: None,
            daily_outbound_messages_limit_multiplier_percent: None,
            overrides_level_limits: false,
        })
    }
}

fn rank_at_least(rank: &str, threshold: &str) -> bool {
    rank_order(rank).unwrap_or_default() >= rank_order(threshold).unwrap_or_default()
}

fn rank_order(rank: &str) -> Option<u8> {
    match rank {
        "F" => Some(0),
        "E" => Some(1),
        "D" => Some(2),
        "C" => Some(3),
        "B" => Some(4),
        "A" => Some(5),
        "S" => Some(6),
        _ => None,
    }
}

fn effective_daily_outbound_messages_limit(
    level_policy: &LevelPolicy,
    rank_policy: &RankPolicy,
) -> Option<i32> {
    match (
        level_policy.daily_outbound_messages_limit,
        rank_policy.daily_outbound_messages_limit_multiplier_percent,
        rank_policy.overrides_level_limits,
    ) {
        (None, _, _) => None,
        (_, None, true) => None,
        (Some(limit), Some(percent), _) => Some((limit * percent) / 100),
        (limit, _, _) => limit,
    }
}

fn effective_daily_attachment_send_limit(level_policy: &LevelPolicy) -> Option<i32> {
    level_policy.daily_attachment_send_limit
}

fn level_progress_percent(
    active_days: i32,
    min_active_days: i32,
    next_level_active_days: Option<i32>,
) -> u8 {
    let Some(next_level_active_days) = next_level_active_days else {
        return 100;
    };

    let span = (next_level_active_days - min_active_days).max(1);
    let progressed = (active_days - min_active_days).clamp(0, span);
    ((progressed * 100) / span) as u8
}

fn seconds_until_next_utc_day(now: DateTime<Utc>) -> i64 {
    let next_day = (now.date_naive() + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("valid midnight");
    (next_day - now.naive_utc()).num_seconds().max(1)
}

fn default_trust_policy() -> TrustPolicyConfig {
    TrustPolicyConfig {
        enforcement: TrustEnforcementConfig::default(),
        daily_counter_retention_days: DEFAULT_DAILY_COUNTER_RETENTION_DAYS,
        score_event_retention_days: DEFAULT_SCORE_EVENT_RETENTION_DAYS,
        level_policies: vec![
            LevelPolicy {
                level: 1,
                min_active_days: 0,
                max_active_days: Some(6),
                daily_outbound_messages_limit: Some(50),
                daily_friend_add_limit: Some(5),
                daily_attachment_send_limit: Some(5),
            },
            LevelPolicy {
                level: 2,
                min_active_days: 7,
                max_active_days: Some(13),
                daily_outbound_messages_limit: Some(100),
                daily_friend_add_limit: Some(10),
                daily_attachment_send_limit: Some(5),
            },
            LevelPolicy {
                level: 3,
                min_active_days: 14,
                max_active_days: Some(29),
                daily_outbound_messages_limit: Some(200),
                daily_friend_add_limit: Some(20),
                daily_attachment_send_limit: Some(10),
            },
            LevelPolicy {
                level: 4,
                min_active_days: 30,
                max_active_days: Some(59),
                daily_outbound_messages_limit: Some(500),
                daily_friend_add_limit: Some(30),
                daily_attachment_send_limit: None,
            },
            LevelPolicy {
                level: 5,
                min_active_days: 60,
                max_active_days: Some(89),
                daily_outbound_messages_limit: Some(1_000),
                daily_friend_add_limit: Some(30),
                daily_attachment_send_limit: None,
            },
            LevelPolicy {
                level: 6,
                min_active_days: 90,
                max_active_days: Some(119),
                daily_outbound_messages_limit: None,
                daily_friend_add_limit: Some(30),
                daily_attachment_send_limit: None,
            },
            LevelPolicy {
                level: 7,
                min_active_days: 120,
                max_active_days: Some(179),
                daily_outbound_messages_limit: None,
                daily_friend_add_limit: Some(50),
                daily_attachment_send_limit: None,
            },
            LevelPolicy {
                level: 8,
                min_active_days: 180,
                max_active_days: None,
                daily_outbound_messages_limit: None,
                daily_friend_add_limit: Some(50),
                daily_attachment_send_limit: None,
            },
        ],
        rank_policies: vec![
            RankPolicy {
                rank: "F".to_string(),
                min_score: 0,
                max_score: Some(99),
                daily_outbound_messages_limit_multiplier_percent: None,
                overrides_level_limits: false,
            },
            RankPolicy {
                rank: "E".to_string(),
                min_score: 100,
                max_score: Some(499),
                daily_outbound_messages_limit_multiplier_percent: None,
                overrides_level_limits: false,
            },
            RankPolicy {
                rank: "D".to_string(),
                min_score: 500,
                max_score: Some(999),
                daily_outbound_messages_limit_multiplier_percent: Some(120),
                overrides_level_limits: false,
            },
            RankPolicy {
                rank: "C".to_string(),
                min_score: 1_000,
                max_score: Some(2_499),
                daily_outbound_messages_limit_multiplier_percent: None,
                overrides_level_limits: false,
            },
            RankPolicy {
                rank: "B".to_string(),
                min_score: 2_500,
                max_score: Some(4_999),
                daily_outbound_messages_limit_multiplier_percent: Some(150),
                overrides_level_limits: false,
            },
            RankPolicy {
                rank: "A".to_string(),
                min_score: 5_000,
                max_score: Some(9_999),
                daily_outbound_messages_limit_multiplier_percent: None,
                overrides_level_limits: true,
            },
            RankPolicy {
                rank: "S".to_string(),
                min_score: 10_000,
                max_score: None,
                daily_outbound_messages_limit_multiplier_percent: None,
                overrides_level_limits: true,
            },
        ],
        community_upvote_daily_cap: 100,
        safe_attachment_types: DEFAULT_SAFE_ATTACHMENT_TYPES
            .iter()
            .map(|entry| (*entry).to_string())
            .collect(),
    }
}

fn load_trust_policy_conn(conn: &mut diesel::PgConnection) -> Result<TrustPolicyConfig, AppError> {
    let raw = admin_settings::table
        .filter(admin_settings::key.eq(admin_service::SETTING_TRUST_POLICY))
        .select(admin_settings::value)
        .first::<String>(conn)
        .optional()?;

    let Some(raw) = raw else {
        return Ok(default_trust_policy());
    };

    match serde_json::from_str::<TrustPolicyConfig>(&raw) {
        Ok(policy) => match normalize_trust_policy(policy) {
            Ok(normalized) => Ok(normalized),
            Err(error) => {
                tracing::warn!(error = %error, "Invalid stored trust policy; using defaults");
                Ok(default_trust_policy())
            }
        },
        Err(error) => {
            tracing::warn!(error = %error, "Could not decode stored trust policy; using defaults");
            Ok(default_trust_policy())
        }
    }
}

fn normalize_trust_policy(mut policy: TrustPolicyConfig) -> Result<TrustPolicyConfig, AppError> {
    if policy.level_policies.is_empty() {
        return Err(AppError::BadRequest(
            "trust policy must define at least one level policy".into(),
        ));
    }
    if policy.rank_policies.is_empty() {
        return Err(AppError::BadRequest(
            "trust policy must define at least one rank policy".into(),
        ));
    }
    if policy.community_upvote_daily_cap < 0 {
        return Err(AppError::BadRequest(
            "community_upvote_daily_cap must be >= 0".into(),
        ));
    }
    if policy.daily_counter_retention_days <= 0 {
        return Err(AppError::BadRequest(
            "daily_counter_retention_days must be > 0".into(),
        ));
    }
    if policy.score_event_retention_days <= 0 {
        return Err(AppError::BadRequest(
            "score_event_retention_days must be > 0".into(),
        ));
    }

    policy.safe_attachment_types = policy
        .safe_attachment_types
        .into_iter()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    policy
        .level_policies
        .sort_by_key(|entry| entry.min_active_days);
    let mut seen_levels = BTreeSet::new();
    for (index, entry) in policy.level_policies.iter().enumerate() {
        if entry.level == 0 || entry.level > 10 {
            return Err(AppError::BadRequest(
                "level policy level must be between 1 and 10".into(),
            ));
        }
        if !seen_levels.insert(entry.level) {
            return Err(AppError::BadRequest(
                "level policy levels must be unique".into(),
            ));
        }
        if entry.min_active_days < 0 {
            return Err(AppError::BadRequest(
                "level policy min_active_days must be >= 0".into(),
            ));
        }
        if let Some(max_active_days) = entry.max_active_days {
            if max_active_days < entry.min_active_days {
                return Err(AppError::BadRequest(
                    "level policy max_active_days must be >= min_active_days".into(),
                ));
            }
            if let Some(next_entry) = policy.level_policies.get(index + 1) {
                if next_entry.min_active_days <= max_active_days {
                    return Err(AppError::BadRequest(
                        "level policy day ranges must not overlap".into(),
                    ));
                }
            }
        } else if index + 1 != policy.level_policies.len() {
            return Err(AppError::BadRequest(
                "only the final level policy may be open-ended".into(),
            ));
        }
        for limit in [
            entry.daily_outbound_messages_limit,
            entry.daily_friend_add_limit,
            entry.daily_attachment_send_limit,
        ]
        .into_iter()
        .flatten()
        {
            if limit < 0 {
                return Err(AppError::BadRequest(
                    "level policy limits must be >= 0".into(),
                ));
            }
        }
    }
    if policy.level_policies[0].min_active_days != 0 {
        return Err(AppError::BadRequest(
            "the first level policy must start at 0 active days".into(),
        ));
    }

    policy.rank_policies.sort_by_key(|entry| entry.min_score);
    let mut seen_ranks = BTreeSet::new();
    for (index, entry) in policy.rank_policies.iter().enumerate() {
        if !matches!(entry.rank.as_str(), "F" | "E" | "D" | "C" | "B" | "A" | "S") {
            return Err(AppError::BadRequest(
                "rank policy rank must be one of F, E, D, C, B, A, S".into(),
            ));
        }
        if !seen_ranks.insert(entry.rank.clone()) {
            return Err(AppError::BadRequest(
                "rank policy ranks must be unique".into(),
            ));
        }
        if entry.min_score < 0 {
            return Err(AppError::BadRequest(
                "rank policy min_score must be >= 0".into(),
            ));
        }
        if let Some(max_score) = entry.max_score {
            if max_score < entry.min_score {
                return Err(AppError::BadRequest(
                    "rank policy max_score must be >= min_score".into(),
                ));
            }
            if let Some(next_entry) = policy.rank_policies.get(index + 1) {
                if next_entry.min_score <= max_score {
                    return Err(AppError::BadRequest(
                        "rank policy score ranges must not overlap".into(),
                    ));
                }
            }
        } else if index + 1 != policy.rank_policies.len() {
            return Err(AppError::BadRequest(
                "only the final rank policy may be open-ended".into(),
            ));
        }
        if let Some(percent) = entry.daily_outbound_messages_limit_multiplier_percent {
            if percent <= 0 {
                return Err(AppError::BadRequest(
                    "rank policy multiplier percent must be > 0".into(),
                ));
            }
        }
    }
    if policy.rank_policies[0].min_score != 0 {
        return Err(AppError::BadRequest(
            "the first rank policy must start at 0 score".into(),
        ));
    }

    Ok(policy)
}

#[cfg(test)]
#[path = "../../tests/unit/trust_unit_tests.rs"]
mod tests;
