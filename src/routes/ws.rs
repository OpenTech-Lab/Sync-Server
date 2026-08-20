use actix_web::{web, HttpRequest, HttpResponse};
use redis::AsyncCommands;
use serde::Deserialize;
use uuid::Uuid;

use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::user_service;
use crate::ws::session::run_ws_session;

// ── Request DTO ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    ticket: String,
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// GET /ws?ticket=<single-use-ticket>
///
/// Upgrades the connection after atomically consuming a short-lived ticket
/// issued by the authenticated `/auth/ws-ticket` endpoint.
pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    query: web::Query<WsQuery>,
    config: web::Data<Config>,
    pool: web::Data<Pool>,
    redis: web::Data<redis::Client>,
) -> Result<HttpResponse, AppError> {
    if query.ticket.len() != 64 || !query.ticket.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::Unauthorized);
    }
    let mut conn = redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
    // GETDEL prevents replay even when the subsequent WebSocket upgrade fails.
    let user_id: Option<String> = conn
        .get_del(format!("ws_ticket:{}", query.ticket))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis getdel: {e}")))?;
    let user_id = user_id
        .and_then(|value| Uuid::parse_str(&value).ok())
        .ok_or(AppError::Unauthorized)?;

    // Ensure the user exists and is active
    user_service::find_by_id(&pool, user_id)?.ok_or(AppError::Unauthorized)?;

    // Best-effort last_seen update (non-fatal if it fails)
    let _ = user_service::update_last_seen(&pool, user_id);

    // Perform the HTTP → WebSocket upgrade
    let (response, session, msg_stream) =
        actix_ws::handle(&req, stream).map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;

    let redis_client = redis.get_ref().clone();
    let db_pool = pool.get_ref().clone();
    // actix_web::rt::spawn does not require Send, which is needed because
    // MessageStream wraps a non-Send payload stream.
    actix_web::rt::spawn(run_ws_session(
        user_id,
        session,
        msg_stream,
        redis_client,
        db_pool,
        config.get_ref().clone(),
    ));

    Ok(response)
}

/// Register the WebSocket route.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::get().to(ws_handler));
}
