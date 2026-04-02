use actix_web::{web, HttpResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{admin_service, moderation_service};

#[derive(Debug, Deserialize)]
pub struct AcceptTermsRequest {
    pub accepted_terms_version: i32,
}

#[derive(Debug, Deserialize)]
pub struct CreateReportRequest {
    pub reported_user_id: Uuid,
    pub source: String,
    pub content_kind: String,
    pub content_id: Option<String>,
    pub reason_code: String,
    pub reporter_note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlockUserRequest {
    pub user_id: Uuid,
    pub reason_code: Option<String>,
    pub reporter_note: Option<String>,
}

pub async fn accept_terms(
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<AcceptTermsRequest>,
) -> Result<HttpResponse, AppError> {
    let safety = moderation_service::record_terms_acceptance(
        &pool,
        auth.0.user_id()?,
        body.accepted_terms_version,
    )?;
    Ok(HttpResponse::Ok().json(safety))
}

pub async fn create_report(
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<CreateReportRequest>,
) -> Result<HttpResponse, AppError> {
    let reporter_user_id = auth.0.user_id()?;
    let report = moderation_service::create_report(
        &pool,
        reporter_user_id,
        moderation_service::ReportInput {
            reported_user_id: body.reported_user_id,
            source: body.source.clone(),
            content_kind: body.content_kind.clone(),
            content_id: body.content_id.clone(),
            reason_code: body.reason_code.clone(),
            reporter_note: body.reporter_note.clone(),
        },
    )?;
    admin_service::append_audit_log(
        &pool,
        Some(reporter_user_id),
        "safety.report.create",
        Some(&report.id.to_string()),
        serde_json::json!({
            "reported_user_id": report.reported_user_id,
            "content_kind": &report.content_kind,
            "source": &report.source,
            "reason_code": &report.reason_code,
        }),
    )?;
    Ok(HttpResponse::Created().json(report))
}

pub async fn list_blocks(pool: web::Data<Pool>, auth: AuthUser) -> Result<HttpResponse, AppError> {
    let blocks = moderation_service::list_blocked_users(&pool, auth.0.user_id()?)?;
    Ok(HttpResponse::Ok().json(blocks))
}

pub async fn block_user(
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<BlockUserRequest>,
) -> Result<HttpResponse, AppError> {
    let blocker_user_id = auth.0.user_id()?;
    let blocked = moderation_service::block_user(
        &pool,
        blocker_user_id,
        body.user_id,
        body.reason_code.clone(),
        body.reporter_note.clone(),
    )?;
    admin_service::append_audit_log(
        &pool,
        Some(blocker_user_id),
        "safety.block.create",
        Some(&body.user_id.to_string()),
        serde_json::json!({
            "reason_code": body.reason_code.as_deref(),
            "reporter_note": body.reporter_note.as_deref(),
        }),
    )?;
    Ok(HttpResponse::Created().json(blocked))
}

pub async fn unblock_user(
    pool: web::Data<Pool>,
    auth: AuthUser,
    user_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let blocker_user_id = auth.0.user_id()?;
    moderation_service::unblock_user(&pool, blocker_user_id, *user_id)?;
    admin_service::append_audit_log(
        &pool,
        Some(blocker_user_id),
        "safety.block.remove",
        Some(&user_id.to_string()),
        serde_json::json!({}),
    )?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/accept-terms", web::post().to(accept_terms))
        .route("/reports", web::post().to(create_report))
        .route("/blocks", web::get().to(list_blocks))
        .route("/blocks", web::post().to(block_user))
        .route("/blocks/{user_id}", web::delete().to(unblock_user));
}
