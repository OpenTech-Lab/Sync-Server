use actix_web::{web, HttpResponse};
use base64::Engine;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::user::UserProfilePublic;
use crate::services::user_service;

const USERNAME_MIN_LEN: usize = 3;
const USERNAME_MAX_LEN: usize = 32;
const AVATAR_MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub username: Option<String>,
    #[serde(default)]
    pub avatar_base64: Option<Option<String>>,
    #[serde(default)]
    pub message_public_key: Option<Option<String>>,
}

fn is_valid_username(value: &str) -> bool {
    let len = value.chars().count();
    if !(USERNAME_MIN_LEN..=USERNAME_MAX_LEN).contains(&len) {
        return false;
    }
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-')
}

fn validate_avatar_base64(avatar_base64: &str) -> Result<(), AppError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(avatar_base64.trim())
        .map_err(|_| AppError::BadRequest("avatar_base64 must be valid base64".into()))?;

    if decoded.len() > AVATAR_MAX_BYTES {
        return Err(AppError::BadRequest("avatar image must be <= 256KB".into()));
    }

    Ok(())
}

fn validate_message_public_key(message_public_key: &str) -> Result<(), AppError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(message_public_key.trim())
        .map_err(|_| AppError::BadRequest("message_public_key must be valid base64".into()))?;
    if decoded.len() != 32 {
        return Err(AppError::BadRequest(
            "message_public_key must decode to 32 bytes".into(),
        ));
    }
    Ok(())
}

pub async fn me(pool: web::Data<Pool>, auth: AuthUser) -> Result<HttpResponse, AppError> {
    let user = user_service::find_by_id(&pool, auth.0.user_id()?)?.ok_or(AppError::Unauthorized)?;
    Ok(HttpResponse::Ok().json(UserProfilePublic::from(user)))
}

pub async fn update_me(
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<UpdateProfileRequest>,
) -> Result<HttpResponse, AppError> {
    let next_username = body
        .username
        .as_ref()
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Some(ref username) = next_username {
        if !is_valid_username(username) {
            return Err(AppError::BadRequest(
                "username must be 3-32 chars and only contain a-zA-Z0-9._-".into(),
            ));
        }
    }

    if let Some(Some(ref avatar)) = body.avatar_base64 {
        validate_avatar_base64(avatar)?;
    }
    if let Some(Some(ref message_public_key)) = body.message_public_key {
        validate_message_public_key(message_public_key)?;
    }

    let updated = user_service::update_profile(
        &pool,
        auth.0.user_id()?,
        next_username,
        body.avatar_base64.clone(),
        body.message_public_key.clone(),
    )?;

    Ok(HttpResponse::Ok().json(UserProfilePublic::from(updated)))
}

pub async fn get_user(
    pool: web::Data<Pool>,
    _auth: AuthUser,
    user_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let user = user_service::find_by_id(&pool, *user_id)?.ok_or(AppError::NotFound)?;
    Ok(HttpResponse::Ok().json(UserProfilePublic::from(user)))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/me", web::get().to(me))
        .route("/me", web::patch().to(update_me))
        .route("/{user_id}", web::get().to(get_user));
}
