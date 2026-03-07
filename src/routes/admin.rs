use actix_web::{web, HttpResponse};
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AdminUser;
use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::server_news::ServerNews;
use crate::models::trust::TrustPolicyConfig;
use crate::services::{admin_service, server_news_service, trust_service};

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
    pub planet_name: Option<String>,
    pub planet_description: Option<String>,
    pub planet_image_base64: Option<String>,
    pub linked_planets: Option<Vec<String>>,
    pub require_approval: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListServerNewsQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateServerNewsRequest {
    pub title: String,
    pub summary: Option<String>,
    pub markdown_content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateServerNewsRequest {
    pub title: String,
    pub summary: Option<String>,
    pub markdown_content: String,
}

#[derive(Debug, serde::Serialize)]
pub struct AdminServerNewsView {
    pub id: Uuid,
    pub title: String,
    pub summary: Option<String>,
    pub markdown_content: String,
    pub created_by: Option<Uuid>,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn to_admin_server_news_view(item: ServerNews) -> AdminServerNewsView {
    AdminServerNewsView {
        id: item.id,
        title: item.title,
        summary: item.summary,
        markdown_content: item.markdown_content,
        created_by: item.created_by,
        published_at: item.published_at,
        updated_at: item.updated_at,
    }
}

const MAX_INPUT_IMAGE_BYTES: usize = 20 * 1024 * 1024;
const MAX_INPUT_IMAGE_ENCODED_CHARS: usize = 28_000_000;
const MAX_OUTPUT_IMAGE_BYTES: usize = 512 * 1024;
const MAX_OUTPUT_DIMENSION: u32 = 1024;
const MIN_OUTPUT_DIMENSION: u32 = 128;

fn parse_supported_image_data_url(data_url: &str) -> Result<(ImageFormat, &str), AppError> {
    if let Some(payload) = data_url.strip_prefix("data:image/png;base64,") {
        return Ok((ImageFormat::Png, payload));
    }
    if let Some(payload) = data_url.strip_prefix("data:image/jpeg;base64,") {
        return Ok((ImageFormat::Jpeg, payload));
    }
    if let Some(payload) = data_url.strip_prefix("data:image/webp;base64,") {
        return Ok((ImageFormat::WebP, payload));
    }
    Err(AppError::BadRequest(
        "planet_image_base64 must be a data URL (png/jpeg/webp)".into(),
    ))
}

fn flatten_with_white_background(img: &DynamicImage) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = ImageBuffer::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let px = rgba.get_pixel(x, y);
            let alpha = px[3] as u16;
            let inv_alpha = 255u16.saturating_sub(alpha);

            let r = ((px[0] as u16 * alpha) + (255u16 * inv_alpha)) / 255u16;
            let g = ((px[1] as u16 * alpha) + (255u16 * inv_alpha)) / 255u16;
            let b = ((px[2] as u16 * alpha) + (255u16 * inv_alpha)) / 255u16;
            rgb.put_pixel(x, y, Rgb([r as u8, g as u8, b as u8]));
        }
    }

    rgb
}

fn encode_jpeg_bytes(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, AppError> {
    let rgb = flatten_with_white_background(img);
    let (width, height) = rgb.dimensions();
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode(&rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|_| AppError::BadRequest("planet_image_base64 could not be encoded".into()))?;
    Ok(out)
}

fn compress_planet_image_to_data_url(data_url: &str) -> Result<String, AppError> {
    let trimmed = data_url.trim();
    if trimmed.len() > MAX_INPUT_IMAGE_ENCODED_CHARS {
        return Err(AppError::BadRequest(
            "planet_image_base64 is too large (max 20MB)".into(),
        ));
    }

    let (source_format, base64_payload) = parse_supported_image_data_url(trimmed)?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(base64_payload)
        .map_err(|_| AppError::BadRequest("planet_image_base64 is not valid base64".into()))?;
    if decoded.len() > MAX_INPUT_IMAGE_BYTES {
        return Err(AppError::BadRequest(
            "planet_image_base64 decoded payload must be <= 20MB".into(),
        ));
    }

    let decoded_image =
        image::load_from_memory_with_format(&decoded, source_format).map_err(|_| {
            AppError::BadRequest("planet_image_base64 could not be decoded as an image".into())
        })?;

    let mut working = if decoded_image.width().max(decoded_image.height()) > MAX_OUTPUT_DIMENSION {
        decoded_image.resize(
            MAX_OUTPUT_DIMENSION,
            MAX_OUTPUT_DIMENSION,
            FilterType::Lanczos3,
        )
    } else {
        decoded_image
    };
    let quality_steps = [85u8, 75u8, 65u8, 55u8, 45u8];
    let mut best: Option<Vec<u8>> = None;

    for _ in 0..4 {
        for quality in quality_steps {
            let encoded = encode_jpeg_bytes(&working, quality)?;
            if encoded.len() <= MAX_OUTPUT_IMAGE_BYTES {
                return Ok(format!(
                    "data:image/jpeg;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(encoded)
                ));
            }
            match &best {
                Some(existing) if existing.len() <= encoded.len() => {}
                _ => best = Some(encoded),
            }
        }

        let next_width = (working.width() * 3 / 4).max(MIN_OUTPUT_DIMENSION);
        let next_height = (working.height() * 3 / 4).max(MIN_OUTPUT_DIMENSION);
        if next_width == working.width() && next_height == working.height() {
            break;
        }
        working = working.resize(next_width, next_height, FilterType::Triangle);
    }

    let fallback = best.ok_or_else(|| {
        AppError::BadRequest("planet_image_base64 could not be compressed".into())
    })?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(fallback)
    ))
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

pub async fn approve_user(
    pool: web::Data<Pool>,
    admin: AdminUser,
    user_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    admin_service::approve_user(&pool, *user_id)?;
    admin_service::append_audit_log(
        &pool,
        Some(admin.0.user_id()?),
        "user.approve",
        Some(&user_id.to_string()),
        serde_json::json!({ "is_approved": true }),
    )?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "approved" })))
}

pub async fn reject_user(
    pool: web::Data<Pool>,
    admin: AdminUser,
    user_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    admin_service::reject_user(&pool, *user_id)?;
    admin_service::append_audit_log(
        &pool,
        Some(admin.0.user_id()?),
        "user.reject",
        Some(&user_id.to_string()),
        serde_json::json!({ "rejected": true }),
    )?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "rejected" })))
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

pub async fn list_server_news(
    pool: web::Data<Pool>,
    _admin: AdminUser,
    query: web::Query<ListServerNewsQuery>,
) -> Result<HttpResponse, AppError> {
    let items = server_news_service::list_news(&pool, query.limit.unwrap_or(50))?;
    Ok(HttpResponse::Ok().json(
        items
            .into_iter()
            .map(to_admin_server_news_view)
            .collect::<Vec<_>>(),
    ))
}

pub async fn create_server_news(
    pool: web::Data<Pool>,
    admin: AdminUser,
    body: web::Json<CreateServerNewsRequest>,
) -> Result<HttpResponse, AppError> {
    let admin_user_id = admin.0.user_id()?;
    let created = server_news_service::create_news(
        &pool,
        admin_user_id,
        &body.title,
        body.summary.as_deref(),
        &body.markdown_content,
    )?;
    admin_service::append_audit_log(
        &pool,
        Some(admin_user_id),
        "server_news.create",
        Some(&created.id.to_string()),
        serde_json::json!({
            "title": created.title,
            "summary_set": created.summary.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
            "markdown_len": created.markdown_content.len(),
        }),
    )?;
    Ok(HttpResponse::Created().json(to_admin_server_news_view(created)))
}

pub async fn update_server_news(
    pool: web::Data<Pool>,
    admin: AdminUser,
    news_id: web::Path<Uuid>,
    body: web::Json<UpdateServerNewsRequest>,
) -> Result<HttpResponse, AppError> {
    let admin_user_id = admin.0.user_id()?;
    let updated = server_news_service::update_news(
        &pool,
        *news_id,
        &body.title,
        body.summary.as_deref(),
        &body.markdown_content,
    )?;
    admin_service::append_audit_log(
        &pool,
        Some(admin_user_id),
        "server_news.update",
        Some(&updated.id.to_string()),
        serde_json::json!({
            "title": updated.title,
            "summary_set": updated.summary.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
            "markdown_len": updated.markdown_content.len(),
        }),
    )?;
    Ok(HttpResponse::Ok().json(to_admin_server_news_view(updated)))
}

pub async fn delete_server_news(
    pool: web::Data<Pool>,
    admin: AdminUser,
    news_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let admin_user_id = admin.0.user_id()?;
    let deleted = server_news_service::delete_news(&pool, *news_id)?;
    admin_service::append_audit_log(
        &pool,
        Some(admin_user_id),
        "server_news.delete",
        Some(&deleted.id.to_string()),
        serde_json::json!({
            "title": deleted.title,
        }),
    )?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "deleted" })))
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
    let mut normalized_linked_planets: Vec<String> = Vec::new();
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
            admin_service::set_setting(&pool, admin_service::SETTING_WEBHOOK_URL, trimmed)?
        }
    } else {
        admin_service::clear_setting(&pool, admin_service::SETTING_WEBHOOK_URL)?;
    }

    if let Some(name) = body.planet_name.as_ref() {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            admin_service::clear_setting(&pool, admin_service::SETTING_PLANET_NAME)?;
        } else {
            admin_service::set_setting(&pool, admin_service::SETTING_PLANET_NAME, trimmed)?;
        }
    } else {
        admin_service::clear_setting(&pool, admin_service::SETTING_PLANET_NAME)?;
    }

    if let Some(description) = body.planet_description.as_ref() {
        let trimmed = description.trim();
        if trimmed.is_empty() {
            admin_service::clear_setting(&pool, admin_service::SETTING_PLANET_DESCRIPTION)?;
        } else {
            admin_service::set_setting(&pool, admin_service::SETTING_PLANET_DESCRIPTION, trimmed)?;
        }
    } else {
        admin_service::clear_setting(&pool, admin_service::SETTING_PLANET_DESCRIPTION)?;
    }

    if let Some(image) = body.planet_image_base64.as_ref() {
        let trimmed = image.trim();
        if trimmed.is_empty() {
            admin_service::clear_setting(&pool, admin_service::SETTING_PLANET_IMAGE_BASE64)?;
        } else {
            let normalized = compress_planet_image_to_data_url(trimmed)?;
            admin_service::set_setting(
                &pool,
                admin_service::SETTING_PLANET_IMAGE_BASE64,
                &normalized,
            )?;
        }
    } else {
        admin_service::clear_setting(&pool, admin_service::SETTING_PLANET_IMAGE_BASE64)?;
    }

    if let Some(linked_planets) = body.linked_planets.as_ref() {
        for raw in linked_planets {
            let trimmed = raw.trim().trim_end_matches('/');
            if trimmed.is_empty() {
                continue;
            }
            let parsed = reqwest::Url::parse(trimmed).map_err(|_| {
                AppError::BadRequest("linked_planets must contain valid URLs".into())
            })?;
            if parsed.scheme() != "https" && parsed.scheme() != "http" {
                return Err(AppError::BadRequest(
                    "linked_planets URLs must use http or https".into(),
                ));
            }
            normalized_linked_planets.push(trimmed.to_string());
        }
        normalized_linked_planets.sort();
        normalized_linked_planets.dedup();

        if normalized_linked_planets.is_empty() {
            admin_service::clear_setting(&pool, admin_service::SETTING_LINKED_PLANETS)?;
        } else {
            let encoded = serde_json::to_string(&normalized_linked_planets)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON encode: {}", e)))?;
            admin_service::set_setting(&pool, admin_service::SETTING_LINKED_PLANETS, &encoded)?;
        }
    } else {
        admin_service::clear_setting(&pool, admin_service::SETTING_LINKED_PLANETS)?;
    }

    if let Some(require_approval) = body.require_approval {
        if require_approval {
            admin_service::set_setting(&pool, admin_service::SETTING_REQUIRE_APPROVAL, "true")?;
        } else {
            admin_service::clear_setting(&pool, admin_service::SETTING_REQUIRE_APPROVAL)?;
        }
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
                .unwrap_or(false),
            "planet_name_set": body
                .planet_name
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
            "planet_description_set": body
                .planet_description
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
            "planet_image_set": body
                .planet_image_base64
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
            "linked_planets_count": normalized_linked_planets.len(),
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

pub async fn get_trust_policy(
    pool: web::Data<Pool>,
    _admin: AdminUser,
) -> Result<HttpResponse, AppError> {
    let policy = trust_service::read_trust_policy(&pool)?;
    Ok(HttpResponse::Ok().json(policy))
}

pub async fn update_trust_policy(
    pool: web::Data<Pool>,
    admin: AdminUser,
    body: web::Json<TrustPolicyConfig>,
) -> Result<HttpResponse, AppError> {
    let policy = trust_service::save_trust_policy(&pool, &body)?;
    admin_service::append_audit_log(
        &pool,
        Some(admin.0.user_id()?),
        "trust_policy.update",
        None,
        serde_json::json!({
            "levels": policy.level_policies.len(),
            "ranks": policy.rank_policies.len(),
            "safe_attachment_types": policy.safe_attachment_types.len(),
            "community_upvote_daily_cap": policy.community_upvote_daily_cap,
        }),
    )?;
    Ok(HttpResponse::Ok().json(policy))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/overview", web::get().to(overview))
        .route("/users", web::get().to(list_users))
        .route("/users/{user_id}/approve", web::post().to(approve_user))
        .route("/users/{user_id}/reject", web::post().to(reject_user))
        .route("/users/{user_id}/suspend", web::post().to(suspend_user))
        .route("/users/{user_id}/activate", web::post().to(activate_user))
        .route("/server-news", web::get().to(list_server_news))
        .route("/server-news", web::post().to(create_server_news))
        .route("/server-news/{news_id}", web::put().to(update_server_news))
        .route(
            "/server-news/{news_id}",
            web::delete().to(delete_server_news),
        )
        .route("/config", web::get().to(get_config))
        .route("/config", web::put().to(update_config))
        .route("/trust-policy", web::get().to(get_trust_policy))
        .route("/trust-policy", web::put().to(update_trust_policy))
        .route("/audit-logs", web::get().to(audit_logs));
}
