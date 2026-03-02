use actix_web::{web, HttpResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AdminUser;
use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::admin_service;

#[derive(Debug, Deserialize)]
pub struct UserQuery {
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub max_users: Option<u32>,
    pub notification_webhook_url: Option<String>,
}

pub async fn overview(pool: web::Data<Pool>, _admin: AdminUser) -> Result<HttpResponse, AppError> {
    let body = admin_service::admin_overview(&pool)?;
    Ok(HttpResponse::Ok().json(body))
}

pub async fn list_users(
    pool: web::Data<Pool>,
    _admin: AdminUser,
    query: web::Query<UserQuery>,
) -> Result<HttpResponse, AppError> {
    let users = admin_service::list_users(&pool, query.q.as_deref())?;
    Ok(HttpResponse::Ok().json(users))
}

pub async fn suspend_user(
    pool: web::Data<Pool>,
    admin: AdminUser,
    user_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    admin_service::set_user_active(&pool, *user_id, false)?;
    admin_service::append_audit_log(
        &pool,
        Some(admin.0.user_id()?),
        "user.suspend",
        Some(&user_id.to_string()),
        serde_json::json!({ "is_active": false }),
    )?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "suspended" })))
}

pub async fn activate_user(
    pool: web::Data<Pool>,
    admin: AdminUser,
    user_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    admin_service::set_user_active(&pool, *user_id, true)?;
    admin_service::append_audit_log(
        &pool,
        Some(admin.0.user_id()?),
        "user.activate",
        Some(&user_id.to_string()),
        serde_json::json!({ "is_active": true }),
    )?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "active" })))
}

pub async fn get_config(
    pool: web::Data<Pool>,
    cfg: web::Data<Config>,
    _admin: AdminUser,
) -> Result<HttpResponse, AppError> {
    let body = admin_service::read_admin_config(&pool, &cfg)?;
    Ok(HttpResponse::Ok().json(body))
}

pub async fn update_config(
    pool: web::Data<Pool>,
    cfg: web::Data<Config>,
    admin: AdminUser,
    body: web::Json<UpdateConfigRequest>,
) -> Result<HttpResponse, AppError> {
    if let Some(max_users) = body.max_users {
        if max_users == 0 {
            return Err(AppError::BadRequest(
                "max_users must be greater than 0".into(),
            ));
        }
        admin_service::set_setting(
            &pool,
            admin_service::SETTING_MAX_USERS,
            &max_users.to_string(),
        )?;
    } else {
        admin_service::clear_setting(&pool, admin_service::SETTING_MAX_USERS)?;
    }

    if let Some(url) = body.notification_webhook_url.as_ref() {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            admin_service::clear_setting(&pool, admin_service::SETTING_WEBHOOK_URL)?;
        } else {
            let parsed = reqwest::Url::parse(trimmed).map_err(|_| {
                AppError::BadRequest("notification_webhook_url must be a valid URL".into())
            })?;
            if parsed.scheme() != "https" {
                return Err(AppError::BadRequest(
                    "notification_webhook_url must use https".into(),
                ));
            }
            admin_service::set_setting(&pool, admin_service::SETTING_WEBHOOK_URL, trimmed)?;
        }
    } else {
        admin_service::clear_setting(&pool, admin_service::SETTING_WEBHOOK_URL)?;
    }

    admin_service::append_audit_log(
        &pool,
        Some(admin.0.user_id()?),
        "config.update",
        None,
        serde_json::json!({
            "max_users": body.max_users,
            "notification_webhook_url_set": body
                .notification_webhook_url
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
        }),
    )?;

    let updated = admin_service::read_admin_config(&pool, &cfg)?;
    Ok(HttpResponse::Ok().json(updated))
}

pub async fn audit_logs(
    pool: web::Data<Pool>,
    _admin: AdminUser,
    query: web::Query<AuditQuery>,
) -> Result<HttpResponse, AppError> {
    let items = admin_service::list_audit_logs(&pool, query.limit.unwrap_or(50))?;
    Ok(HttpResponse::Ok().json(items))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/overview", web::get().to(overview))
        .route("/users", web::get().to(list_users))
        .route("/users/{user_id}/suspend", web::post().to(suspend_user))
        .route("/users/{user_id}/activate", web::post().to(activate_user))
        .route("/config", web::get().to(get_config))
        .route("/config", web::put().to(update_config))
        .route("/audit-logs", web::get().to(audit_logs));
}
