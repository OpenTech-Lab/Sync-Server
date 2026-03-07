use actix_web::{web, HttpResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::trust::TrustSnapshot;
use crate::routes::federation;
use crate::services::user_service;
use crate::services::{
    admin_service, message_service, push_dispatch_service, redis_pubsub, trust_service,
};

// ── Request DTOs ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub recipient_id: String,
    pub recipient_server_url: Option<String>,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ConversationQuery {
    pub before: Option<Uuid>,
    pub limit: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveContactRequest {
    pub recipient_id: String,
    pub recipient_server_url: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ResolveContactResponse {
    pub partner_id: Uuid,
    pub recipient_id: String,
    pub recipient_server_url: String,
    pub display_handle: String,
}

#[derive(Debug, serde::Serialize)]
struct MessageLimitExceededResponse {
    error: &'static str,
    code: &'static str,
    retry_after_seconds: i64,
    trust: TrustSnapshot,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

fn instance_host(instance_domain: &str) -> String {
    if let Ok(parsed) = reqwest::Url::parse(instance_domain) {
        if let Some(host) = parsed.host_str() {
            return host.to_lowercase();
        }
    }
    instance_domain
        .split(':')
        .next()
        .unwrap_or(instance_domain)
        .to_lowercase()
}

fn normalize_server_url(raw: &str) -> Result<reqwest::Url, AppError> {
    let parsed = reqwest::Url::parse(raw.trim())
        .map_err(|e| AppError::BadRequest(format!("Invalid recipient_server_url: {e}")))?;
    if parsed.host_str().is_none() {
        return Err(AppError::BadRequest(
            "recipient_server_url must include host".into(),
        ));
    }
    Ok(parsed)
}

fn canonical_server_url(url: &reqwest::Url) -> String {
    match url.port() {
        Some(port) => format!(
            "{}://{}:{port}",
            url.scheme(),
            url.host_str().unwrap_or_default()
        ),
        None => format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default()),
    }
}

async fn publish_new_message_event(
    redis: &redis::Client,
    sender_id: Uuid,
    recipient_id: Uuid,
    message: &crate::models::message::Message,
) {
    let event = serde_json::json!({ "type": "new_message", "message": message });
    if let Ok(mut conn) = redis_pubsub::get_async_conn(redis).await {
        let _ = redis_pubsub::publish(&mut conn, &redis_pubsub::user_channel(recipient_id), &event)
            .await;
        let _ =
            redis_pubsub::publish(&mut conn, &redis_pubsub::user_channel(sender_id), &event).await;
    }
}

fn parse_local_uuid(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw.trim())
        .map_err(|_| AppError::BadRequest("recipient_id must be a UUID for local chats".into()))
}

async fn dispatch_push_for_message(
    pool: &Pool,
    cfg: &Config,
    message: &crate::models::message::Message,
) {
    let push_pool = pool.clone();
    let push_cfg = cfg.clone();
    let push_sender_id = message.sender_id;
    let push_recipient_id = message.recipient_id;
    let push_message_id = message.id;
    let push_content = message.content.clone();
    actix_web::rt::spawn(async move {
        if let Err(error) = push_dispatch_service::dispatch_new_message(
            &push_pool,
            &push_cfg,
            push_recipient_id,
            push_sender_id,
            push_message_id,
            &push_content,
        )
        .await
        {
            match &error {
                AppError::Internal(cause) => {
                    tracing::warn!(
                        error = %error,
                        cause = %cause,
                        error_debug = ?error,
                        "Push dispatch failed"
                    );
                }
                _ => {
                    tracing::warn!(error = %error, error_debug = ?error, "Push dispatch failed");
                }
            }
        }
    });
}

pub async fn resolve_contact(
    cfg: web::Data<Config>,
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<ResolveContactRequest>,
) -> Result<HttpResponse, AppError> {
    let _ = auth.0.user_id()?;

    let recipient_id = body.recipient_id.trim().to_lowercase();
    if recipient_id.is_empty() {
        return Err(AppError::BadRequest("recipient_id cannot be empty".into()));
    }

    let server_url = normalize_server_url(&body.recipient_server_url)?;
    let canonical = canonical_server_url(&server_url);
    let local_instance_host = instance_host(&cfg.instance_domain);
    let target_host = server_url.host_str().unwrap_or_default().to_lowercase();

    if target_host == local_instance_host {
        let local_id = parse_local_uuid(&recipient_id)?;
        let user = user_service::find_by_id(&pool, local_id)?.ok_or(AppError::NotFound)?;
        return Ok(HttpResponse::Ok().json(ResolveContactResponse {
            partner_id: user.id,
            recipient_id,
            recipient_server_url: canonical,
            display_handle: user.username,
        }));
    }

    let shadow = user_service::ensure_federated_shadow_user(&pool, &recipient_id, &canonical)?;
    Ok(HttpResponse::Ok().json(ResolveContactResponse {
        partner_id: shadow.id,
        recipient_id: recipient_id.clone(),
        recipient_server_url: canonical,
        display_handle: format!("{}@{}", recipient_id, target_host),
    }))
}

/// POST /api/messages → 201 Message
pub async fn send_message(
    cfg: web::Data<Config>,
    pool: web::Data<Pool>,
    redis: web::Data<redis::Client>,
    auth: AuthUser,
    body: web::Json<SendMessageRequest>,
) -> Result<HttpResponse, AppError> {
    let sender_id = auth.0.user_id()?;

    if body.content.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Message content cannot be empty".into(),
        ));
    }

    let content = body.content.trim().to_string();
    let local_instance_host = instance_host(&cfg.instance_domain);
    let explicit_remote_target = match body.recipient_server_url.as_ref() {
        Some(raw) if !raw.trim().is_empty() => {
            let parsed = normalize_server_url(raw)?;
            let host = parsed.host_str().unwrap_or_default().to_lowercase();
            Some((canonical_server_url(&parsed), host))
        }
        _ => None,
    };

    let recipient_id_raw = body.recipient_id.trim().to_lowercase();
    if recipient_id_raw.is_empty() {
        return Err(AppError::BadRequest("recipient_id cannot be empty".into()));
    }

    let recipient_user = if let Some((remote_server_url, remote_host)) = explicit_remote_target {
        if remote_host == local_instance_host {
            let local_id = parse_local_uuid(&recipient_id_raw)?;
            user_service::find_by_id(&pool, local_id)?.ok_or(AppError::NotFound)?
        } else {
            user_service::ensure_federated_shadow_user(
                &pool,
                &recipient_id_raw,
                &remote_server_url,
            )?
        }
    } else {
        let local_id = parse_local_uuid(&recipient_id_raw)?;
        user_service::find_by_id(&pool, local_id)?.ok_or(AppError::NotFound)?
    };

    let message =
        match trust_service::send_message_with_trust(&pool, sender_id, recipient_user.id, content)?
        {
            trust_service::SendMessageWithTrustResult::Sent { message } => message,
            trust_service::SendMessageWithTrustResult::Limited {
                trust,
                retry_after_seconds,
            } => {
                admin_service::append_audit_log(
                    &pool,
                    Some(sender_id),
                    "trust.blocked_action.outbound_message_limit",
                    Some(&recipient_user.id.to_string()),
                    serde_json::json!({
                        "retry_after_seconds": retry_after_seconds,
                        "level": trust.level,
                        "rank": trust.rank,
                        "daily_outbound_messages_limit": trust.daily_outbound_messages_limit,
                        "daily_outbound_messages_sent": trust.daily_outbound_messages_sent,
                    }),
                )?;
                return Ok(
                    HttpResponse::TooManyRequests().json(MessageLimitExceededResponse {
                        error: "Daily outbound message limit reached for your current trust level.",
                        code: "daily_message_limit_reached",
                        retry_after_seconds,
                        trust,
                    }),
                );
            }
        };
    publish_new_message_event(&redis, sender_id, recipient_user.id, &message).await;

    let federated_target = user_service::federated_identity_for_user(&recipient_user);
    if let Some(target) = federated_target {
        let server_url = body
            .recipient_server_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("https://{}", target.remote_host));

        federation::deliver_direct_message_to_remote(
            &cfg,
            &pool,
            sender_id,
            &target.remote_user_id,
            &server_url,
            &message.content,
        )
        .await?;
    } else {
        dispatch_push_for_message(pool.get_ref(), cfg.get_ref(), &message).await;
    }

    Ok(HttpResponse::Created().json(message))
}

/// GET /api/messages/unread-counts → 200 { "<uuid>": count, … }
///
/// Must be registered *before* `/{partner_id}` so the literal segment wins.
pub async fn unread_counts(
    pool: web::Data<Pool>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    let viewer_id = auth.0.user_id()?;
    let counts = message_service::unread_counts(&pool, viewer_id)?;
    Ok(HttpResponse::Ok().json(counts))
}

/// GET /api/messages/{partner_id}?before=<UUID>&limit=<u8>
pub async fn get_conversation(
    pool: web::Data<Pool>,
    auth: AuthUser,
    partner_id: web::Path<Uuid>,
    query: web::Query<ConversationQuery>,
) -> Result<HttpResponse, AppError> {
    let viewer_id = auth.0.user_id()?;
    let limit = query.limit.unwrap_or(50);

    let messages =
        message_service::get_conversation(&pool, viewer_id, *partner_id, query.before, limit)?;

    Ok(HttpResponse::Ok().json(messages))
}

/// POST /api/messages/{partner_id}/read → 200 { "count": usize }
pub async fn mark_read(
    pool: web::Data<Pool>,
    auth: AuthUser,
    partner_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let viewer_id = auth.0.user_id()?;
    let count = message_service::mark_read(&pool, viewer_id, *partner_id)?;
    trust_service::record_human_activity(&pool, viewer_id)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "count": count })))
}

/// Register all message routes under a given service config.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg
        // POST /api/messages — registered on the scope root
        .route("", web::post().to(send_message))
        .route("/resolve-contact", web::post().to(resolve_contact))
        // Literal segment must come before the path-param catch-all
        .route("/unread-counts", web::get().to(unread_counts))
        .route("/{partner_id}", web::get().to(get_conversation))
        .route("/{partner_id}/read", web::post().to(mark_read));
}
