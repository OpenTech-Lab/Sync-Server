use actix_web::{web, HttpResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{message_service, push_dispatch_service, redis_pubsub};

// ── Request DTOs ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub recipient_id: Uuid,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ConversationQuery {
    pub before: Option<Uuid>,
    pub limit: Option<u8>,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// POST /api/messages → 201 Message
pub async fn send_message(
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

    let message =
        message_service::send_message(&pool, sender_id, body.recipient_id, body.content.clone())?;

    // Publish to both users' channels for real-time delivery (best-effort).
    let event = serde_json::json!({ "type": "new_message", "message": &message });

    if let Ok(mut conn) = redis_pubsub::get_async_conn(&redis).await {
        let _ = redis_pubsub::publish(
            &mut conn,
            &redis_pubsub::user_channel(body.recipient_id),
            &event,
        )
        .await;
        let _ =
            redis_pubsub::publish(&mut conn, &redis_pubsub::user_channel(sender_id), &event).await;
    }

    // Push dispatch is best-effort and asynchronous; REST write succeeds even
    // if webhook delivery fails.
    let push_pool = pool.get_ref().clone();
    let push_sender_id = sender_id;
    let push_recipient_id = body.recipient_id;
    let push_message_id = message.id;
    let push_content = message.content.clone();
    actix_web::rt::spawn(async move {
        if let Err(error) = push_dispatch_service::dispatch_new_message(
            &push_pool,
            push_recipient_id,
            push_sender_id,
            push_message_id,
            &push_content,
        )
        .await
        {
            tracing::warn!(error = %error, "Push dispatch failed");
        }
    });

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
    Ok(HttpResponse::Ok().json(serde_json::json!({ "count": count })))
}

/// Register all message routes under a given service config.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg
        // POST /api/messages — registered on the scope root
        .route("", web::post().to(send_message))
        // Literal segment must come before the path-param catch-all
        .route("/unread-counts", web::get().to(unread_counts))
        .route("/{partner_id}", web::get().to(get_conversation))
        .route("/{partner_id}/read", web::post().to(mark_read));
}
