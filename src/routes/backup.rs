use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::backup_service;

#[derive(Debug, Deserialize)]
pub struct UpsertBackupRequest {
    pub encrypted_blob: String,
}

#[derive(Debug, serde::Serialize)]
pub struct BackupResponse {
    pub encrypted_blob: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn upsert_backup(
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<UpsertBackupRequest>,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let payload = body.encrypted_blob.trim();
    if payload.is_empty() {
        return Err(AppError::BadRequest(
            "encrypted_blob cannot be empty".into(),
        ));
    }

    let backup = backup_service::upsert_backup(&pool, user_id, payload)?;
    Ok(HttpResponse::Ok().json(BackupResponse {
        encrypted_blob: backup.encrypted_blob,
        updated_at: backup.updated_at,
    }))
}

pub async fn get_backup(pool: web::Data<Pool>, auth: AuthUser) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let maybe_backup = backup_service::get_backup(&pool, user_id)?;
    let backup = maybe_backup.ok_or(AppError::NotFound)?;
    Ok(HttpResponse::Ok().json(BackupResponse {
        encrypted_blob: backup.encrypted_blob,
        updated_at: backup.updated_at,
    }))
}

pub async fn delete_backup(
    pool: web::Data<Pool>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let _ = backup_service::delete_backup(&pool, user_id)?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::put().to(upsert_backup))
        .route("", web::get().to(get_backup))
        .route("", web::delete().to(delete_backup));
}
