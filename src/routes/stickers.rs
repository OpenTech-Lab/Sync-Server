use actix_web::{web, HttpResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::{AdminUser, AuthUser};
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::admin_service;
use crate::services::sticker_service;

#[derive(Debug, Deserialize)]
pub struct UploadStickerRequest {
    pub group_name: String,
    pub name: String,
    pub mime_type: String,
    pub content_base64: String,
}

#[derive(Debug, Deserialize)]
pub struct ModerateStickerRequest {
    pub action: String,
}

pub async fn upload(
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<UploadStickerRequest>,
) -> Result<HttpResponse, AppError> {
    let requester_id = auth.0.user_id()?;
    let created = sticker_service::upload_sticker(
        &pool,
        requester_id,
        &auth.0.role,
        sticker_service::UploadStickerInput {
            group_name: body.group_name.clone(),
            name: body.name.clone(),
            mime_type: body.mime_type.clone(),
            content_base64: body.content_base64.clone(),
        },
    )?;

    admin_service::append_audit_log(
        &pool,
        Some(requester_id),
        "sticker.upload",
        Some(&created.id.to_string()),
        serde_json::json!({
            "group_name": created.group_name,
            "mime_type": created.mime_type,
            "size_bytes": created.size_bytes,
            "status": created.status,
        }),
    )?;

    Ok(HttpResponse::Created().json(created))
}

pub async fn list(pool: web::Data<Pool>, auth: AuthUser) -> Result<HttpResponse, AppError> {
    let requester_id = auth.0.user_id()?;
    let items = sticker_service::list_stickers(&pool, requester_id, &auth.0.role)?;
    Ok(HttpResponse::Ok().json(items))
}

pub async fn get_by_id(
    pool: web::Data<Pool>,
    auth: AuthUser,
    sticker_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let requester_id = auth.0.user_id()?;
    let sticker = sticker_service::get_sticker(&pool, requester_id, &auth.0.role, *sticker_id)?;
    Ok(HttpResponse::Ok().json(sticker))
}

pub async fn moderate(
    pool: web::Data<Pool>,
    admin: AdminUser,
    sticker_id: web::Path<Uuid>,
    body: web::Json<ModerateStickerRequest>,
) -> Result<HttpResponse, AppError> {
    let updated = sticker_service::moderate_sticker(&pool, *sticker_id, body.action.trim())?;

    admin_service::append_audit_log(
        &pool,
        Some(admin.0.user_id()?),
        "sticker.moderate",
        Some(&sticker_id.to_string()),
        serde_json::json!({
            "action": body.action,
            "status": updated.status,
        }),
    )?;

    Ok(HttpResponse::Ok().json(updated))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/upload", web::post().to(upload))
        .route("/list", web::get().to(list))
        .route("/{id}", web::get().to(get_by_id))
        .route("/{id}/moderate", web::post().to(moderate));
}
