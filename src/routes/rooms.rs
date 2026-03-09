use actix_web::{web, HttpResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::room::RoomMessage;
use crate::services::{guild_service, push_dispatch_service, redis_pubsub, room_service};

#[derive(Debug, Deserialize)]
pub struct CreateRoomRequest {
    pub name: String,
    #[serde(default)]
    pub member_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct RoomMessagesQuery {
    pub before: Option<Uuid>,
    pub limit: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct SendRoomMessageRequest {
    pub content: String,
}

async fn publish_new_room_message_event(
    redis: &redis::Client,
    room_id: Uuid,
    member_ids: &[Uuid],
    message: &RoomMessage,
) {
    let event = serde_json::json!({
        "type": "new_room_message",
        "room_id": room_id,
        "message": message,
    });
    if let Ok(mut conn) = redis_pubsub::get_async_conn(redis).await {
        for member_id in member_ids {
            let channel = redis_pubsub::user_channel(*member_id);
            let _ = redis_pubsub::publish(&mut conn, &channel, &event).await;
        }
    }
}

fn dispatch_push_for_room_message(
    pool: &Pool,
    cfg: &Config,
    room_member_ids: Vec<Uuid>,
    sender_id: Uuid,
    message_id: Uuid,
    message_content: String,
) {
    let push_pool = pool.clone();
    let push_cfg = cfg.clone();
    actix_web::rt::spawn(async move {
        for member_id in room_member_ids {
            if member_id == sender_id {
                continue;
            }
            if let Err(error) = push_dispatch_service::dispatch_new_message(
                &push_pool,
                &push_cfg,
                member_id,
                sender_id,
                message_id,
                &message_content,
            )
            .await
            {
                tracing::warn!(
                    room_member_id = %member_id,
                    sender_id = %sender_id,
                    error = %error,
                    "Room push dispatch failed"
                );
            }
        }
    });
}

pub async fn create_room(
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<CreateRoomRequest>,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let room = room_service::create_room(&pool, user_id, &body.name, &body.member_ids)?;
    guild_service::record_human_activity(&pool, user_id)?;
    Ok(HttpResponse::Created().json(room))
}

pub async fn list_rooms(pool: web::Data<Pool>, auth: AuthUser) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let rooms = room_service::list_rooms(&pool, user_id)?;
    Ok(HttpResponse::Ok().json(rooms))
}

pub async fn get_room(
    pool: web::Data<Pool>,
    auth: AuthUser,
    room_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let room = room_service::get_room(&pool, *room_id, user_id)?;
    Ok(HttpResponse::Ok().json(room))
}

pub async fn get_room_messages(
    pool: web::Data<Pool>,
    auth: AuthUser,
    room_id: web::Path<Uuid>,
    query: web::Query<RoomMessagesQuery>,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let limit = query.limit.unwrap_or(50);
    let messages = room_service::get_room_messages(&pool, *room_id, user_id, query.before, limit)?;
    Ok(HttpResponse::Ok().json(messages))
}

pub async fn send_room_message(
    cfg: web::Data<Config>,
    pool: web::Data<Pool>,
    redis: web::Data<redis::Client>,
    auth: AuthUser,
    room_id: web::Path<Uuid>,
    body: web::Json<SendRoomMessageRequest>,
) -> Result<HttpResponse, AppError> {
    let sender_id = auth.0.user_id()?;
    let content = body.content.trim().to_string();
    if content.is_empty() {
        return Err(AppError::BadRequest(
            "Message content cannot be empty".into(),
        ));
    }

    let message = room_service::send_room_message(&pool, *room_id, sender_id, &content)?;
    let room_member_ids = room_service::list_room_member_ids(&pool, *room_id, sender_id)?;
    publish_new_room_message_event(&redis, *room_id, &room_member_ids, &message).await;
    dispatch_push_for_room_message(
        pool.get_ref(),
        cfg.get_ref(),
        room_member_ids,
        sender_id,
        message.id,
        content,
    );
    guild_service::record_human_activity(&pool, sender_id)?;
    Ok(HttpResponse::Created().json(message))
}

pub async fn mark_room_read(
    pool: web::Data<Pool>,
    auth: AuthUser,
    room_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let count = room_service::mark_room_read(&pool, *room_id, user_id)?;
    guild_service::record_human_activity(&pool, user_id)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "count": count })))
}

pub async fn leave_room(
    pool: web::Data<Pool>,
    auth: AuthUser,
    room_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    room_service::leave_room(&pool, *room_id, user_id)?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn delete_room(
    pool: web::Data<Pool>,
    auth: AuthUser,
    room_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    room_service::delete_room(&pool, *room_id, user_id)?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::post().to(create_room))
        .route("", web::get().to(list_rooms))
        .route("/{room_id}", web::get().to(get_room))
        .route("/{room_id}", web::delete().to(delete_room))
        .route("/{room_id}/messages", web::get().to(get_room_messages))
        .route("/{room_id}/messages", web::post().to(send_room_message))
        .route("/{room_id}/read", web::post().to(mark_room_read))
        .route("/{room_id}/leave", web::post().to(leave_room));
}
