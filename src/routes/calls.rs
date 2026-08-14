use actix_web::{web, HttpResponse};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use redis::AsyncCommands;
use ring::hmac;
use serde::Serialize;
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{call_service, redis_pubsub};

const TURN_CREDENTIAL_TTL_SECS: u64 = 3600;

async fn call_history(
    pool: web::Data<Pool>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let records = call_service::history_for_user(&pool, user_id, 50)?;
    Ok(HttpResponse::Ok().json(records))
}

#[derive(Serialize)]
struct IceServerEntry {
    urls: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    credential: Option<String>,
}

#[derive(Serialize)]
struct IceServersResponse {
    ice_servers: Vec<IceServerEntry>,
}

async fn ice_servers(
    config: web::Data<Config>,
    _auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    let mut servers = vec![
        IceServerEntry { urls: "stun:stun.l.google.com:19302".into(), username: None, credential: None },
        IceServerEntry { urls: "stun:stun1.l.google.com:19302".into(), username: None, credential: None },
    ];

    if let Some(secret) = &config.turn_secret {
        // TURN REST API: username = expiry timestamp, credential = HMAC-SHA1(secret, username).
        // coturn verifies this with --use-auth-secret so the shared secret never reaches clients.
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + TURN_CREDENTIAL_TTL_SECS;
        let username = expiry.to_string();
        let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret.as_bytes());
        let tag = hmac::sign(&key, username.as_bytes());
        let credential = B64.encode(tag.as_ref());

        // UDP TURN — works on most networks
        servers.push(IceServerEntry {
            urls: format!("turn:{}:3478", config.instance_domain),
            username: Some(username.clone()),
            credential: Some(credential.clone()),
        });
        // TURNS over TLS/TCP — fallback for networks that block UDP or non-443 TCP
        servers.push(IceServerEntry {
            urls: format!("turns:{}:5349?transport=tcp", config.instance_domain),
            username: Some(username),
            credential: Some(credential),
        });
    }

    Ok(HttpResponse::Ok().json(IceServersResponse { ice_servers: servers }))
}

#[derive(Serialize)]
struct CallOfferResponse {
    call_id: Uuid,
    sdp_offer: String,
}

/// Returns the stashed SDP offer for `call_id` and deletes it from Redis
/// (GETDEL semantics), so it can only be consumed once. Only the call's
/// callee may fetch it. This exists so a device woken by a PushKit VoIP
/// push (with no live WebSocket connection to have received the offer via
/// pubsub) can still retrieve it to answer the call.
async fn get_call_offer(
    pool: web::Data<Pool>,
    redis_client: web::Data<redis::Client>,
    auth: AuthUser,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let call_id = path.into_inner();

    let record = call_service::find(&pool, call_id)?.ok_or(AppError::NotFound)?;
    if record.callee_id != user_id {
        return Err(AppError::Forbidden);
    }

    let mut conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis connection error: {e}")))?;
    let key = redis_pubsub::call_offer_key(call_id);
    let sdp_offer: Option<String> = conn
        .get_del(&key)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis GETDEL error: {e}")))?;
    let sdp_offer = sdp_offer.filter(|s| !s.is_empty()).ok_or(AppError::NotFound)?;

    Ok(HttpResponse::Ok().json(CallOfferResponse { call_id, sdp_offer }))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::get().to(call_history));
    cfg.route("/ice-servers", web::get().to(ice_servers));
    cfg.route("/offer/{call_id}", web::get().to(get_call_offer));
}
