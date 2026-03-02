use diesel::prelude::*;
use uuid::Uuid;

use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::admin::{AdminAuditLog, AdminSetting, NewAdminAuditLog, NewAdminSetting};
use crate::models::federation::FederationDelivery;
use crate::models::user::User;
use crate::schema::admin_audit_logs::dsl as audit_dsl;
use crate::schema::admin_settings::dsl as setting_dsl;
use crate::schema::federation_deliveries::dsl as fed_delivery_dsl;
use crate::schema::users::dsl as user_dsl;

pub const SETTING_MAX_USERS: &str = "max_users";
pub const SETTING_WEBHOOK_URL: &str = "notification_webhook_url";

#[derive(Debug, serde::Serialize)]
pub struct AdminOverview {
    pub system_status: &'static str,
    pub total_users: i64,
    pub active_users: i64,
    pub admin_users: i64,
    pub federation_pending: i64,
    pub federation_failed: i64,
    pub federation_dead_letter: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct AdminConfigView {
    pub max_users_override: Option<u32>,
    pub effective_max_users: Option<u32>,
    pub notification_webhook_url: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct AdminUserView {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<User> for AdminUserView {
    fn from(value: User) -> Self {
        Self {
            id: value.id,
            username: value.username,
            email: value.email,
            role: value.role,
            is_active: value.is_active,
            created_at: value.created_at,
            last_seen_at: value.last_seen_at,
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
        federation_pending,
        federation_failed,
        federation_dead_letter,
    })
}

pub fn list_users(pool: &Pool, query: Option<&str>) -> Result<Vec<AdminUserView>, AppError> {
    let mut conn = pool.get()?;
    let mut q = user_dsl::users.into_boxed();

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

    let users = q
        .order(user_dsl::created_at.desc())
        .select(User::as_select())
        .load::<User>(&mut conn)?;

    Ok(users.into_iter().map(AdminUserView::from).collect())
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

    Ok(AdminConfigView {
        max_users_override,
        effective_max_users: max_users_override.or(config.max_users),
        notification_webhook_url,
    })
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
