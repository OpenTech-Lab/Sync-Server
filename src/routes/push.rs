use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::push_token_service;

const TOKEN_MIN_LEN: usize = 16;
const TOKEN_MAX_LEN: usize = 1024;

#[derive(Debug, Deserialize)]
pub struct PushTokenRequest {
    pub token: String,
    pub platform: Option<String>,
}

fn normalize_platform(raw: Option<&str>) -> Result<String, AppError> {
    let normalized = raw.unwrap_or("unknown").trim().to_lowercase();
    match normalized.as_str() {
        "ios" | "android" | "macos" | "windows" | "linux" | "web" | "unknown" => Ok(normalized),
        _ => Err(AppError::BadRequest("Unsupported platform".into())),
    }
}

fn validate_token(raw: &str) -> Result<String, AppError> {
    let token = raw.trim().to_string();
    let len = token.chars().count();
    if !(TOKEN_MIN_LEN..=TOKEN_MAX_LEN).contains(&len) {
        return Err(AppError::BadRequest("Push token length is invalid".into()));
    }
    Ok(token)
}

pub async fn register(
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<PushTokenRequest>,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let token = validate_token(&body.token)?;
    let platform = normalize_platform(body.platform.as_deref())?;

    push_token_service::upsert_token(&pool, user_id, &platform, &token)?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn unregister(
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<PushTokenRequest>,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let token = validate_token(&body.token)?;

    let _ = push_token_service::unregister_token(&pool, user_id, &token)?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/token", web::post().to(register))
        .route("/token", web::delete().to(unregister));
}

#[cfg(test)]
mod tests {
    use super::normalize_platform;

    #[test]
    fn normalize_platform_accepts_known_values() {
        assert_eq!(normalize_platform(Some("iOS")).unwrap(), "ios");
        assert_eq!(normalize_platform(Some("android")).unwrap(), "android");
        assert_eq!(normalize_platform(None).unwrap(), "unknown");
    }

    #[test]
    fn normalize_platform_rejects_invalid_values() {
        assert!(normalize_platform(Some("beeper")).is_err());
    }
}
