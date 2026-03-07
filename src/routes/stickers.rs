use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{AdminUser, AuthUser};
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::admin_service;
use crate::services::{sticker_service, trust_service};

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

#[derive(Debug, Serialize)]
struct StickerTrustBlockedResponse {
    error: String,
    code: &'static str,
    trust: crate::models::trust::TrustSnapshot,
    allowed_mime_types: Vec<String>,
}

#[derive(Debug, Serialize)]
struct StickerTrustLimitedResponse {
    error: String,
    code: &'static str,
    retry_after_seconds: i64,
    trust: crate::models::trust::TrustSnapshot,
    allowed_mime_types: Vec<String>,
}

pub async fn upload(
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<UploadStickerRequest>,
) -> Result<HttpResponse, AppError> {
    let requester_id = auth.0.user_id()?;
    let input = sticker_service::UploadStickerInput {
        group_name: body.group_name.clone(),
        name: body.name.clone(),
        mime_type: body.mime_type.clone(),
        content_base64: body.content_base64.clone(),
    };
    let trust_mime_type = input.mime_type.clone();
    let created = if auth.0.role == "admin" {
        sticker_service::upload_sticker(&pool, requester_id, &auth.0.role, input)?
    } else {
        match trust_service::run_attachment_action_with_trust(
            &pool,
            requester_id,
            &trust_mime_type,
            |conn| sticker_service::upload_sticker_conn(conn, requester_id, &auth.0.role, input),
        )? {
            trust_service::AttachmentActionWithTrustResult::Completed { value } => value,
            trust_service::AttachmentActionWithTrustResult::Limited {
                trust,
                retry_after_seconds,
            } => {
                let allowed_mime_types = sticker_upload_allowed_mime_types(&trust);
                admin_service::append_audit_log(
                    &pool,
                    Some(requester_id),
                    "trust.blocked_action.attachment_daily_limit",
                    None,
                    serde_json::json!({
                        "retry_after_seconds": retry_after_seconds,
                        "level": trust.level,
                        "rank": trust.rank,
                        "daily_attachment_send_limit": trust.daily_attachment_send_limit,
                        "daily_attachment_sends_sent": trust.daily_attachment_sends_sent,
                        "mime_type": &trust_mime_type,
                    }),
                )?;
                return Ok(
                    HttpResponse::TooManyRequests().json(StickerTrustLimitedResponse {
                        error:
                            "Daily attachment upload limit reached for your current trust level."
                                .to_string(),
                        code: "daily_attachment_limit_reached",
                        retry_after_seconds,
                        trust,
                        allowed_mime_types,
                    }),
                );
            }
            trust_service::AttachmentActionWithTrustResult::UnsupportedMime { trust } => {
                let allowed_mime_types = sticker_upload_allowed_mime_types(&trust);
                admin_service::append_audit_log(
                    &pool,
                    Some(requester_id),
                    "trust.blocked_action.attachment_type_not_allowed",
                    None,
                    serde_json::json!({
                        "level": trust.level,
                        "rank": trust.rank,
                        "mime_type": &trust_mime_type,
                        "allowed_mime_types": &allowed_mime_types,
                    }),
                )?;
                return Ok(HttpResponse::Forbidden().json(StickerTrustBlockedResponse {
                    error: "This file type is not currently allowed for your trust tier."
                        .to_string(),
                    code: "attachment_type_not_allowed",
                    trust,
                    allowed_mime_types,
                }));
            }
        }
    };

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

fn sticker_upload_allowed_mime_types(trust: &crate::models::trust::TrustSnapshot) -> Vec<String> {
    sticker_service::supported_mime_types()
        .iter()
        .filter(|mime_type| {
            trust
                .allowed_attachment_types
                .iter()
                .any(|allowed| allowed == **mime_type)
        })
        .map(|mime_type| (*mime_type).to_string())
        .collect()
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
    let admin_user_id = admin.0.user_id()?;
    let updated = sticker_service::moderate_sticker(&pool, *sticker_id, body.action.trim())?;
    let moderation_score = trust_service::award_validated_moderation_action(
        &pool,
        admin_user_id,
        trust_service::EVENT_VALIDATED_MODERATION_STICKER_REVIEW,
        Some(&sticker_id.to_string()),
        serde_json::json!({
            "action": body.action.trim(),
            "sticker_id": sticker_id.to_string(),
            "uploader_id": updated.uploader_id,
        }),
    )?;

    admin_service::append_audit_log(
        &pool,
        Some(admin_user_id),
        "sticker.moderate",
        Some(&sticker_id.to_string()),
        serde_json::json!({
            "action": body.action,
            "status": updated.status,
            "score_event_id": moderation_score.event.as_ref().map(|event| event.id),
            "score_delta_applied": moderation_score.applied_delta,
            "contribution_score": moderation_score.contribution_score,
            "derived_rank": moderation_score.derived_rank,
            "score_duplicate": moderation_score.duplicate,
            "score_suppressed_reason": moderation_score.suppressed_reason,
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
