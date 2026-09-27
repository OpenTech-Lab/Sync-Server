use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;

use crate::auth::tokens::verify_access_token;
use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{redis_pubsub, user_service};
use crate::ws::session::run_ws_session;

// ── Request DTO ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    token: String,
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// GET /ws?token=<jwt>
///
/// Upgrades the connection to WebSocket after verifying the bearer token
/// supplied in the query string (headers cannot carry custom values during
/// the browser WS handshake).
pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    query: web::Query<WsQuery>,
    config: web::Data<Config>,
    pool: web::Data<Pool>,
    redis: web::Data<redis::Client>,
) -> Result<HttpResponse, AppError> {
    // Validate the JWT from the query parameter
    let claims = verify_access_token(&query.token, &config.jwt_secret)
        .map_err(|_| AppError::Unauthorized)?;

    let user_id = claims.user_id()?;

    // Ensure the user exists and is active
    user_service::find_by_id(&pool, user_id)?.ok_or(AppError::Unauthorized)?;

    // Best-effort last_seen update (non-fatal if it fails)
    let _ = user_service::update_last_seen(&pool, user_id);

    // Subscribe before upgrading: pubsub keeps nothing for absent subscribers,
    // so anything published between the 101 and a later SUBSCRIBE (a message,
    // an incoming call) would be lost while the client believes it is live.
    let channel = redis_pubsub::user_channel(user_id);
    let pubsub = redis_pubsub::subscribe(redis.get_ref(), &[channel.as_str()])
        .await
        .map_err(AppError::Internal)?;

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
        pubsub,
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
