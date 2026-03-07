use diesel::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::admin::{AdminAuditLog, AdminSetting, NewAdminAuditLog, NewAdminSetting};
use crate::models::federation::FederationDelivery;
use crate::models::trust::{TrustScoreEvent, UserTrustStats};
use crate::models::user::User;
use crate::schema::admin_audit_logs::dsl as audit_dsl;
use crate::schema::admin_settings::dsl as setting_dsl;
use crate::schema::federation_deliveries::dsl as fed_delivery_dsl;
use crate::schema::trust_score_events::dsl as score_event_dsl;
use crate::schema::user_trust_stats::dsl as trust_dsl;
use crate::schema::users::dsl as user_dsl;

pub const SETTING_MAX_USERS: &str = "max_users";
pub const SETTING_WEBHOOK_URL: &str = "notification_webhook_url";
pub const SETTING_PLANET_NAME: &str = "planet_name";
pub const SETTING_PLANET_DESCRIPTION: &str = "planet_description";
pub const SETTING_PLANET_IMAGE_BASE64: &str = "planet_image_base64";
pub const SETTING_LINKED_PLANETS: &str = "linked_planets";
pub const SETTING_REQUIRE_APPROVAL: &str = "registration_requires_approval";
pub const SETTING_TRUST_POLICY: &str = "trust_policy";

#[derive(Debug, serde::Serialize)]
pub struct AdminOverview {
    pub system_status: &'static str,
    pub total_users: i64,
    pub active_users: i64,
    pub admin_users: i64,
    pub pending_approval: i64,
    pub trust_challenged: i64,
    pub trust_frozen: i64,
    pub federation_pending: i64,
    pub federation_failed: i64,
    pub federation_dead_letter: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct AdminConfigView {
    pub max_users_override: Option<u32>,
    pub effective_max_users: Option<u32>,
    pub notification_webhook_url: Option<String>,
    pub planet_name: Option<String>,
    pub planet_description: Option<String>,
    pub planet_image_base64: Option<String>,
    pub linked_planets: Vec<String>,
    pub require_approval: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct AdminBlockedActionCount {
    pub action: String,
    pub count: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct AdminTrustReviewMetrics {
    pub current_challenged_users: i64,
    pub current_frozen_users: i64,
    pub challenged_transitions: i64,
    pub frozen_transitions: i64,
    pub recovery_transitions: i64,
    pub likely_false_positive_recoveries: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct AdminUserView {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub is_active: bool,
    pub is_approved: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub trust: Option<AdminTrustReviewView>,
}

#[derive(Debug, serde::Serialize)]
pub struct AdminTrustReviewView {
    pub active_days: i32,
    pub derived_level: i32,
    pub derived_rank: String,
    pub automation_review_state: String,
    pub suspicious_activity_streak: i32,
    pub last_human_activity_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_active_day: Option<chrono::NaiveDate>,
}

impl AdminUserView {
    fn from_parts(value: User, trust: Option<UserTrustStats>) -> Self {
        Self {
            id: value.id,
            username: value.username,
            email: value.email,
            role: value.role,
            is_active: value.is_active,
            is_approved: value.is_approved,
            created_at: value.created_at,
            last_seen_at: value.last_seen_at,
            trust: trust.map(|trust| AdminTrustReviewView {
                active_days: trust.active_days,
                derived_level: trust.derived_level,
                derived_rank: trust.derived_rank,
                automation_review_state: trust.automation_review_state,
                suspicious_activity_streak: trust.suspicious_activity_streak,
                last_human_activity_at: trust.last_human_activity_at,
                last_active_day: trust.last_active_day,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrustReviewStateFilter {
    Challenged,
    Frozen,
    AnyFlagged,
}

impl TrustReviewStateFilter {
    fn parse(raw: Option<&str>) -> Result<Self, AppError> {
        match raw.map(|value| value.trim().to_lowercase()) {
            None => Ok(Self::AnyFlagged),
            Some(state) if state.is_empty() || state == "flagged" => Ok(Self::AnyFlagged),
            Some(state) if state == "challenged" => Ok(Self::Challenged),
            Some(state) if state == "frozen" => Ok(Self::Frozen),
            Some(_) => Err(AppError::BadRequest(
                "automation_review_state must be one of challenged, frozen, flagged".into(),
            )),
        }
    }
}

pub fn admin_overview(pool: &Pool) -> Result<AdminOverview, AppError> {
    let mut conn = pool.get()?;

    let total_users = user_dsl::users.count().get_result(&mut conn)?;
    let active_users = user_dsl::users
        .filter(user_dsl::is_active.eq(true))
        .count()
        .get_result(&mut conn)?;
    let admin_users = user_dsl::users
        .filter(user_dsl::role.eq("admin"))
        .count()
        .get_result(&mut conn)?;
    let pending_approval = user_dsl::users
        .filter(user_dsl::is_approved.eq(false))
        .count()
        .get_result(&mut conn)?;
    let trust_challenged = trust_dsl::user_trust_stats
        .filter(trust_dsl::automation_review_state.eq("challenged"))
        .count()
        .get_result(&mut conn)?;
    let trust_frozen = trust_dsl::user_trust_stats
        .filter(trust_dsl::automation_review_state.eq("frozen"))
        .count()
        .get_result(&mut conn)?;

    let deliveries = fed_delivery_dsl::federation_deliveries
        .select(FederationDelivery::as_select())
        .load::<FederationDelivery>(&mut conn)
        .unwrap_or_default();

    let mut federation_pending = 0i64;
    let mut federation_failed = 0i64;
    let mut federation_dead_letter = 0i64;

    for item in deliveries {
        match item.status.as_str() {
            "pending" | "retrying" => federation_pending += 1,
            "failed" => federation_failed += 1,
            "dead_letter" => federation_dead_letter += 1,
            _ => {}
        }
    }

    Ok(AdminOverview {
        system_status: "ok",
        total_users,
        active_users,
        admin_users,
        pending_approval,
        trust_challenged,
        trust_frozen,
        federation_pending,
        federation_failed,
        federation_dead_letter,
    })
}

pub fn list_users(
    pool: &Pool,
    query: Option<&str>,
    automation_review_state: Option<&str>,
) -> Result<Vec<AdminUserView>, AppError> {
    let mut conn = pool.get()?;
    let mut q = user_dsl::users
        .left_join(trust_dsl::user_trust_stats.on(trust_dsl::user_id.eq(user_dsl::id)))
        .into_boxed();

    if let Some(raw) = query {
        let needle = format!("%{}%", raw.trim().to_lowercase());
        if !raw.trim().is_empty() {
            q = q.filter(
                user_dsl::username
                    .ilike(needle.clone())
                    .or(user_dsl::email.ilike(needle)),
            );
        }
    }

    if let Some(raw_state) = automation_review_state {
        let state = raw_state.trim().to_lowercase();
        if !state.is_empty() {
            q = match state.as_str() {
                "clear" => q.filter(
                    trust_dsl::automation_review_state
                        .eq("clear")
                        .or(trust_dsl::user_id.is_null()),
                ),
                "challenged" | "frozen" => q.filter(trust_dsl::automation_review_state.eq(state)),
                _ => {
                    return Err(AppError::BadRequest(
                        "automation_review_state must be one of clear, challenged, frozen".into(),
                    ));
                }
            };
        }
    }

    let users = q
        .order(user_dsl::created_at.desc())
        .select((User::as_select(), Option::<UserTrustStats>::as_select()))
        .load::<(User, Option<UserTrustStats>)>(&mut conn)?;

    Ok(users
        .into_iter()
        .map(|(user, trust)| AdminUserView::from_parts(user, trust))
        .collect())
}

pub fn list_trust_review_users(
    pool: &Pool,
    automation_review_state: Option<&str>,
    limit: i64,
) -> Result<Vec<AdminUserView>, AppError> {
    let mut conn = pool.get()?;
    let safe_limit = limit.clamp(1, 200);
    let review_state = TrustReviewStateFilter::parse(automation_review_state)?;

    let mut q = user_dsl::users
        .inner_join(trust_dsl::user_trust_stats.on(trust_dsl::user_id.eq(user_dsl::id)))
        .into_boxed();

    q = match review_state {
        TrustReviewStateFilter::Challenged => {
            q.filter(trust_dsl::automation_review_state.eq("challenged"))
        }
        TrustReviewStateFilter::Frozen => q.filter(trust_dsl::automation_review_state.eq("frozen")),
        TrustReviewStateFilter::AnyFlagged => q.filter(
            trust_dsl::automation_review_state
                .eq("challenged")
                .or(trust_dsl::automation_review_state.eq("frozen")),
        ),
    };

    let users = q
        .order((
            trust_dsl::suspicious_activity_streak.desc(),
            user_dsl::created_at.desc(),
        ))
        .limit(safe_limit)
        .select((User::as_select(), UserTrustStats::as_select()))
        .load::<(User, UserTrustStats)>(&mut conn)?;

    Ok(users
        .into_iter()
        .map(|(user, trust)| AdminUserView::from_parts(user, Some(trust)))
        .collect())
}

pub fn approve_user(pool: &Pool, user_id: Uuid) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    let changed = diesel::update(user_dsl::users.find(user_id))
        .set((user_dsl::is_approved.eq(true), user_dsl::is_active.eq(true)))
        .execute(&mut conn)?;
    if changed == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub fn reject_user(pool: &Pool, user_id: Uuid) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    let changed = diesel::delete(user_dsl::users.find(user_id)).execute(&mut conn)?;
    if changed == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub fn is_approval_required(pool: &Pool) -> Result<bool, AppError> {
    let setting = get_setting(pool, SETTING_REQUIRE_APPROVAL)?;
    Ok(setting.map(|s| s.value == "true").unwrap_or(false))
}

pub fn set_user_active(pool: &Pool, user_id: Uuid, active: bool) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    let changed = diesel::update(user_dsl::users.find(user_id))
        .set(user_dsl::is_active.eq(active))
        .execute(&mut conn)?;

    if changed == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub fn get_setting(pool: &Pool, key: &str) -> Result<Option<AdminSetting>, AppError> {
    let mut conn = pool.get()?;
    setting_dsl::admin_settings
        .filter(setting_dsl::key.eq(key))
        .select(AdminSetting::as_select())
        .first::<AdminSetting>(&mut conn)
        .optional()
        .map_err(AppError::from)
}

pub fn set_setting(pool: &Pool, key: &str, value: &str) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    diesel::insert_into(setting_dsl::admin_settings)
        .values(&NewAdminSetting {
            key: key.to_string(),
            value: value.to_string(),
        })
        .on_conflict(setting_dsl::key)
        .do_update()
        .set((
            setting_dsl::value.eq(value),
            setting_dsl::updated_at.eq(chrono::Utc::now()),
        ))
        .execute(&mut conn)?;
    Ok(())
}

pub fn clear_setting(pool: &Pool, key: &str) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    diesel::delete(setting_dsl::admin_settings.filter(setting_dsl::key.eq(key)))
        .execute(&mut conn)?;
    Ok(())
}

pub fn effective_max_users(pool: &Pool, config: &Config) -> Result<Option<u32>, AppError> {
    let override_value =
        get_setting(pool, SETTING_MAX_USERS)?.and_then(|s| s.value.parse::<u32>().ok());

    Ok(override_value.or(config.max_users))
}

pub fn read_admin_config(pool: &Pool, config: &Config) -> Result<AdminConfigView, AppError> {
    let max_users_override =
        get_setting(pool, SETTING_MAX_USERS)?.and_then(|s| s.value.parse::<u32>().ok());
    let notification_webhook_url = get_setting(pool, SETTING_WEBHOOK_URL)?.map(|s| s.value);
    let planet_name = get_setting(pool, SETTING_PLANET_NAME)?.map(|s| s.value);
    let planet_description = get_setting(pool, SETTING_PLANET_DESCRIPTION)?.map(|s| s.value);
    let planet_image_base64 = get_setting(pool, SETTING_PLANET_IMAGE_BASE64)?.map(|s| s.value);
    let linked_planets = read_linked_planets(pool)?;

    let require_approval = get_setting(pool, SETTING_REQUIRE_APPROVAL)?
        .map(|s| s.value == "true")
        .unwrap_or(false);

    Ok(AdminConfigView {
        max_users_override,
        effective_max_users: max_users_override.or(config.max_users),
        notification_webhook_url,
        planet_name,
        planet_description,
        planet_image_base64,
        linked_planets,
        require_approval,
    })
}

pub fn read_linked_planets(pool: &Pool) -> Result<Vec<String>, AppError> {
    let raw = get_setting(pool, SETTING_LINKED_PLANETS)?.map(|s| s.value);
    let Some(raw) = raw else {
        return Ok(vec![]);
    };
    let parsed = serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default();
    Ok(parsed
        .into_iter()
        .map(|item| item.trim().trim_end_matches('/').to_string())
        .filter(|item| !item.is_empty())
        .collect())
}

pub fn append_audit_log(
    pool: &Pool,
    actor_user_id: Option<Uuid>,
    action: &str,
    target: Option<&str>,
    details: serde_json::Value,
) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    diesel::insert_into(audit_dsl::admin_audit_logs)
        .values(&NewAdminAuditLog {
            id: Uuid::new_v4(),
            actor_user_id,
            action: action.to_string(),
            target: target.map(ToString::to_string),
            details,
        })
        .execute(&mut conn)?;
    Ok(())
}

pub fn list_audit_logs(pool: &Pool, limit: i64) -> Result<Vec<AdminAuditLog>, AppError> {
    let mut conn = pool.get()?;
    let safe_limit = limit.clamp(1, 200);

    audit_dsl::admin_audit_logs
        .order(audit_dsl::created_at.desc())
        .limit(safe_limit)
        .select(AdminAuditLog::as_select())
        .load::<AdminAuditLog>(&mut conn)
        .map_err(AppError::from)
}

pub fn list_trust_score_events(
    pool: &Pool,
    user_id: Option<Uuid>,
    event_type: Option<&str>,
    limit: i64,
) -> Result<Vec<TrustScoreEvent>, AppError> {
    let mut conn = pool.get()?;
    let safe_limit = limit.clamp(1, 200);
    let mut q = score_event_dsl::trust_score_events.into_boxed();

    if let Some(user_id) = user_id {
        q = q.filter(score_event_dsl::user_id.eq(user_id));
    }

    if let Some(event_type) = event_type.map(str::trim).filter(|value| !value.is_empty()) {
        q = q.filter(score_event_dsl::event_type.eq(event_type));
    }

    q.order(score_event_dsl::created_at.desc())
        .limit(safe_limit)
        .select(TrustScoreEvent::as_select())
        .load::<TrustScoreEvent>(&mut conn)
        .map_err(AppError::from)
}

pub fn list_trust_blocked_action_counts(
    pool: &Pool,
    limit: i64,
) -> Result<Vec<AdminBlockedActionCount>, AppError> {
    let mut conn = pool.get()?;
    let safe_limit = limit.clamp(1, 50) as usize;
    let rows = audit_dsl::admin_audit_logs
        .filter(audit_dsl::action.like("trust.blocked_action.%"))
        .order(audit_dsl::created_at.desc())
        .select(AdminAuditLog::as_select())
        .load::<AdminAuditLog>(&mut conn)?;

    let mut counts = HashMap::<String, i64>::new();
    for row in rows {
        let normalized = row
            .action
            .strip_prefix("trust.blocked_action.")
            .unwrap_or(&row.action)
            .to_string();
        *counts.entry(normalized).or_insert(0) += 1;
    }

    let mut summary = counts
        .into_iter()
        .map(|(action, count)| AdminBlockedActionCount { action, count })
        .collect::<Vec<_>>();
    summary.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.action.cmp(&right.action))
    });
    summary.truncate(safe_limit);
    Ok(summary)
}

pub fn trust_review_metrics(pool: &Pool) -> Result<AdminTrustReviewMetrics, AppError> {
    let mut conn = pool.get()?;
    let current_challenged_users = trust_dsl::user_trust_stats
        .filter(trust_dsl::automation_review_state.eq("challenged"))
        .count()
        .get_result(&mut conn)?;
    let current_frozen_users = trust_dsl::user_trust_stats
        .filter(trust_dsl::automation_review_state.eq("frozen"))
        .count()
        .get_result(&mut conn)?;

    let rows = audit_dsl::admin_audit_logs
        .filter(audit_dsl::action.eq("trust.review_state.changed"))
        .order(audit_dsl::created_at.desc())
        .select(AdminAuditLog::as_select())
        .load::<AdminAuditLog>(&mut conn)?;

    let mut challenged_transitions = 0i64;
    let mut frozen_transitions = 0i64;
    let mut recovery_transitions = 0i64;

    for row in rows {
        let new_state = row
            .details
            .get("new_state")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let previous_state = row
            .details
            .get("previous_state")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        match new_state {
            "challenged" => challenged_transitions += 1,
            "frozen" => frozen_transitions += 1,
            "clear" if matches!(previous_state, "challenged" | "frozen") => {
                recovery_transitions += 1
            }
            _ => {}
        }
    }

    Ok(AdminTrustReviewMetrics {
        current_challenged_users,
        current_frozen_users,
        challenged_transitions,
        frozen_transitions,
        recovery_transitions,
        likely_false_positive_recoveries: recovery_transitions,
    })
}
