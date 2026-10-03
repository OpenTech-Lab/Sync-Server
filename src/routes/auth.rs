use actix_web::{web, HttpRequest, HttpResponse};
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use chrono::Utc;
use diesel::prelude::*;
use redis::AsyncCommands;
use ring::agreement;
use ring::digest::{digest, SHA256};
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

use crate::auth::claims::Claims;
use crate::auth::legacy;
use crate::auth::password::{hash_password, verify_password};
use crate::auth::tokens::{generate_refresh_token, hash_token, issue_access_token};
use crate::auth::AuthUser;
use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::refresh_token::{NewRefreshToken, RefreshToken};
use crate::models::user::{NewUser, UserPublic};
use crate::schema::refresh_tokens::dsl as rt_dsl;
use crate::schema::users::dsl as user_dsl;
use crate::services::{admin_service, email_service, moderation_service, user_service};

// ── Request / Response DTOs ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub altcha_payload: Option<String>,
    pub accepted_terms_version: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub altcha_payload: Option<String>,
    pub accepted_terms_version: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct DeviceLoginRequest {
    pub device_auth_pubkey: String,
    /// `challenge_id` and `proof` come as a pair (see `DEVICE_LOGIN_CHALLENGE_TTL_SECS`).
    /// Both absent = legacy body from installed apps <= 1.3.2.
    pub challenge_id: Option<String>,
    pub proof: Option<String>,
    pub altcha_payload: Option<String>,
    pub accepted_terms_version: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct DeviceLoginChallengeRequest {
    pub device_auth_pubkey: String,
}

#[derive(Debug, Serialize)]
pub struct DeviceLoginChallengeResponse {
    pub challenge_id: String,
    pub server_ephemeral_pubkey: String,
    pub expires_in: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeviceLoginChallengeRecord {
    device_auth_pubkey: String,
    proof_key: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct SetupAdminRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub setup_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    /// Access token lifetime in seconds.
    pub expires_in: u64,
}

#[derive(Debug, Serialize)]
pub struct SetupStatusResponse {
    pub needs_setup: bool,
}

#[derive(Debug, Serialize)]
pub struct QrLoginCreateResponse {
    pub session_id: String,
    pub qr_payload: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
pub struct QrLoginSessionPath {
    pub session_id: String,
}

/// Legacy `GET ...?secret=` poll (installed apps <= 1.3.2).
#[derive(Debug, Deserialize)]
pub struct QrLoginSessionQuery {
    pub secret: String,
}

#[derive(Debug, Deserialize)]
pub struct QrLoginPollRequest {
    pub secret: String,
}

#[derive(Debug, Serialize)]
pub struct WsTicketResponse {
    pub ticket: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
pub struct QrLoginApproveRequest {
    pub session_id: String,
    pub secret: String,
}

#[derive(Debug, Serialize)]
pub struct QrLoginStatusResponse {
    pub status: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QrLoginSessionRecord {
    secret: String,
    status: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

const QR_LOGIN_TTL_SECS: u64 = 30;
const WS_TICKET_TTL_SECS: u64 = 30;
const QR_LOGIN_STATUS_PENDING: &str = "pending";
const QR_LOGIN_STATUS_APPROVED: &str = "approved";
const QR_LOGIN_STATUS_EXPIRED: &str = "expired";
const DEVICE_LOGIN_CHALLENGE_TTL_SECS: u64 = 60;

static QR_LOGIN_FALLBACK_STORE: OnceLock<Mutex<HashMap<String, (QrLoginSessionRecord, i64)>>> =
    OnceLock::new();
static ADMIN_SETUP_TOKEN: OnceLock<Mutex<Option<String>>> = OnceLock::new();

/// Per-IP new-account creation timestamps (Unix seconds) for the sliding
/// registration rate limiter.  Key = client IP string.
static REGISTRATION_LIMITER: OnceLock<Mutex<HashMap<String, Vec<i64>>>> = OnceLock::new();

/// Maximum new accounts that can be created from one IP address per hour.
const REGISTRATION_MAX_PER_HOUR: usize = 5;

fn registration_limiter() -> &'static Mutex<HashMap<String, Vec<i64>>> {
    REGISTRATION_LIMITER.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Extract the real client IP, respecting `X-Forwarded-For` set by nginx.
fn extract_client_ip(req: &HttpRequest) -> String {
    if let Some(xff) = req.headers().get("X-Forwarded-For") {
        if let Ok(val) = xff.to_str() {
            if let Some(first) = val.split(',').next() {
                return first.trim().to_string();
            }
        }
    }
    if let Some(xri) = req.headers().get("X-Real-IP") {
        if let Ok(val) = xri.to_str() {
            return val.trim().to_string();
        }
    }
    req.peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Returns `true` when the IP has exceeded the hourly registration limit.
fn is_registration_rate_limited(ip: &str) -> bool {
    let window = 3600_i64; // 1 hour
    let now = Utc::now().timestamp();
    let Ok(mut store) = registration_limiter().lock() else {
        return false; // fail-open on lock poisoning
    };
    let timestamps = store.entry(ip.to_string()).or_default();
    // Keep only timestamps within the window.
    timestamps.retain(|&ts| now - ts < window);
    timestamps.len() >= REGISTRATION_MAX_PER_HOUR
}

/// Record a successful new-account creation from this IP.
fn record_registration(ip: &str) {
    let now = Utc::now().timestamp();
    let Ok(mut store) = registration_limiter().lock() else {
        return;
    };
    store.entry(ip.to_string()).or_default().push(now);
}

fn decode_device_public_key(encoded: &str) -> Result<Vec<u8>, AppError> {
    let decoded = BASE64_STANDARD
        .decode(encoded.trim())
        .map_err(|_| AppError::BadRequest("device_auth_pubkey must be valid base64".into()))?;
    if decoded.len() != 32 {
        return Err(AppError::BadRequest(
            "device_auth_pubkey must encode a 32-byte X25519 public key".into(),
        ));
    }
    Ok(decoded)
}

fn device_login_proof_message(challenge_id: &str, device_auth_pubkey: &str) -> String {
    format!(
        "device-login:{}:{}",
        challenge_id,
        device_auth_pubkey.trim()
    )
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn is_valid_username(value: &str) -> bool {
    let normalized = value.trim();
    let len = normalized.chars().count();
    if !(2..=32).contains(&len) {
        return false;
    }
    normalized.chars().all(|ch| !ch.is_control())
}

/// Verifies an ALTCHA payload (base64-encoded JSON) against the server HMAC key.
///
/// The client widget encodes the solved payload as `Base64(JSON{...})`.  We
/// decode and then delegate to `altcha_lib_rs::verify_json_solution` which
/// checks the HMAC signature and the optional expiry timestamp.
fn verify_altcha(payload: &str, hmac_key: &str) -> bool {
    let Ok(decoded) = BASE64_STANDARD.decode(payload.trim()) else {
        return false;
    };
    let Ok(json_str) = std::str::from_utf8(&decoded) else {
        return false;
    };
    altcha_lib_rs::verify_json_solution(json_str, hmac_key, true).is_ok()
}

fn qr_login_redis_key(session_id: &str) -> String {
    format!("qr_login:{}", session_id)
}

fn qr_login_fallback_store() -> &'static Mutex<HashMap<String, (QrLoginSessionRecord, i64)>> {
    QR_LOGIN_FALLBACK_STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn admin_setup_token_store() -> &'static Mutex<Option<String>> {
    ADMIN_SETUP_TOKEN.get_or_init(|| Mutex::new(None))
}

fn random_setup_token() -> Result<String, AppError> {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    const GROUPS: usize = 6;
    const GROUP_LEN: usize = 4;
    const TOKEN_LEN: usize = GROUPS * GROUP_LEN;

    let mut raw = [0u8; TOKEN_LEN];
    let rng = ring::rand::SystemRandom::new();
    rng.fill(&mut raw)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("RNG failed")))?;

    let mut token = String::with_capacity(TOKEN_LEN + (GROUPS - 1));
    for (idx, byte) in raw.iter().enumerate() {
        if idx > 0 && idx % GROUP_LEN == 0 {
            token.push('-');
        }
        let ch = ALPHABET[(*byte as usize) % ALPHABET.len()] as char;
        token.push(ch);
    }

    Ok(token)
}

fn current_setup_token() -> Option<String> {
    admin_setup_token_store()
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn set_setup_token(next_token: Option<String>) {
    if let Ok(mut guard) = admin_setup_token_store().lock() {
        *guard = next_token;
    }
}

fn is_setup_token_valid(token: &str) -> bool {
    let Some(expected) = current_setup_token() else {
        return false;
    };
    expected == token
}

fn clear_setup_token() {
    set_setup_token(None);
}

pub fn initialize_first_admin_setup_link(pool: &Pool, config: &Config) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    let admin_count: i64 = user_dsl::users
        .filter(user_dsl::role.eq("admin"))
        .count()
        .get_result(&mut conn)?;

    if admin_count > 0 {
        clear_setup_token();
        return Ok(());
    }

    let setup_token = random_setup_token()?;
    set_setup_token(Some(setup_token.clone()));

    let scheme = if config.enforce_https {
        "https"
    } else {
        "http"
    };
    let setup_url = format!(
        "{scheme}://{}/setup-admin/{setup_token}",
        config.instance_domain
    );
    tracing::warn!(
        setup_url = %setup_url,
        "No admin account found. Use this one-time setup URL to create the first admin."
    );
    println!("\n[SYNC] One-time admin setup URL: {setup_url}\n");

    Ok(())
}

fn fallback_save_qr_login_record(session_id: &str, record: &QrLoginSessionRecord, ttl_secs: u64) {
    let expires_at = Utc::now().timestamp() + ttl_secs as i64;
    if let Ok(mut map) = qr_login_fallback_store().lock() {
        map.insert(session_id.to_string(), (record.clone(), expires_at));
    }
}

fn fallback_load_qr_login_record(session_id: &str) -> Option<QrLoginSessionRecord> {
    let Ok(mut map) = qr_login_fallback_store().lock() else {
        return None;
    };
    let Some((record, expires_at)) = map.get(session_id).cloned() else {
        return None;
    };
    if Utc::now().timestamp() > expires_at {
        map.remove(session_id);
        return None;
    }
    Some(record)
}

fn fallback_delete_qr_login_record(session_id: &str) {
    if let Ok(mut map) = qr_login_fallback_store().lock() {
        map.remove(session_id);
    }
}

async fn load_qr_login_record(
    redis: &redis::Client,
    session_id: &str,
) -> Result<Option<QrLoginSessionRecord>, AppError> {
    let mut conn = match redis.get_multiplexed_async_connection().await {
        Ok(conn) => conn,
        Err(_) => return Ok(fallback_load_qr_login_record(session_id)),
    };
    let key = qr_login_redis_key(session_id);
    let raw: Option<String> = match conn.get(&key).await {
        Ok(raw) => raw,
        Err(_) => return Ok(fallback_load_qr_login_record(session_id)),
    };
    let Some(raw) = raw else {
        return Ok(fallback_load_qr_login_record(session_id));
    };
    let parsed = serde_json::from_str::<QrLoginSessionRecord>(&raw)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis JSON parse: {}", e)))?;
    Ok(Some(parsed))
}

async fn save_qr_login_record(
    redis: &redis::Client,
    session_id: &str,
    record: &QrLoginSessionRecord,
    ttl_secs: u64,
) -> Result<(), AppError> {
    let raw = serde_json::to_string(record)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis JSON encode: {}", e)))?;
    let mut conn = match redis.get_multiplexed_async_connection().await {
        Ok(conn) => conn,
        Err(_) => {
            fallback_save_qr_login_record(session_id, record, ttl_secs);
            return Ok(());
        }
    };
    let key = qr_login_redis_key(session_id);
    if conn.set_ex::<_, _, ()>(&key, raw, ttl_secs).await.is_err() {
        fallback_save_qr_login_record(session_id, record, ttl_secs);
    }
    Ok(())
}

async fn delete_qr_login_record(redis: &redis::Client, session_id: &str) -> Result<(), AppError> {
    let mut conn = match redis.get_multiplexed_async_connection().await {
        Ok(conn) => conn,
        Err(_) => {
            fallback_delete_qr_login_record(session_id);
            return Ok(());
        }
    };
    let key = qr_login_redis_key(session_id);
    if conn.del::<_, ()>(&key).await.is_err() {
        fallback_delete_qr_login_record(session_id);
    }
    Ok(())
}

/// Issue a fresh access + refresh token pair, storing the refresh token in DB.
fn mint_tokens(
    conn: &mut crate::db::DbConn,
    user_id: Uuid,
    role: &str,
    family: Uuid,
    config: &Config,
) -> Result<TokenResponse, AppError> {
    let now = Utc::now();
    let exp = now.timestamp() + config.jwt_access_expiry_secs as i64;
    let claims = Claims::new(user_id, role.to_string(), now.timestamp(), exp);
    let access_token =
        issue_access_token(&claims, &config.jwt_secret).map_err(AppError::Internal)?;

    let (raw_refresh, hash) = generate_refresh_token();
    let expires_at = now + chrono::Duration::seconds(config.jwt_refresh_expiry_secs as i64);

    diesel::insert_into(crate::schema::refresh_tokens::table)
        .values(&NewRefreshToken {
            id: Uuid::new_v4(),
            user_id,
            token_hash: hash,
            family,
            expires_at,
        })
        .execute(conn)?;

    Ok(TokenResponse {
        access_token,
        refresh_token: raw_refresh,
        expires_in: config.jwt_access_expiry_secs,
    })
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// POST /auth/register → 201 UserPublic
pub async fn register(
    req: HttpRequest,
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    body: web::Json<RegisterRequest>,
) -> Result<HttpResponse, AppError> {
    let client_ip = extract_client_ip(&req);
    if is_registration_rate_limited(&client_ip) {
        return Err(AppError::TooManyRequests(
            "Too many accounts created from this IP. Try again later.".into(),
        ));
    }
    // Validate minimal input
    if body.username.trim().is_empty() || body.email.trim().is_empty() || body.password.len() < 8 {
        return Err(AppError::BadRequest(
            "username, email required; password must be ≥8 chars".into(),
        ));
    }
    if !is_valid_username(&body.username) {
        return Err(AppError::BadRequest(
            "username must be 2-32 characters (no control characters)".into(),
        ));
    }

    if let Some(hmac_key) = &config.altcha_hmac_key {
        let payload = body.altcha_payload.as_deref().unwrap_or("");
        if payload.is_empty() {
            return Err(AppError::BadRequest("altcha_payload is required".into()));
        }
        if !verify_altcha(payload, hmac_key) {
            return Err(AppError::Unauthorized);
        }
    }

    let pw_hash = hash_password(body.password.clone()).await?;

    let approval_required = admin_service::is_approval_required(&pool)?;
    let new_user = NewUser {
        id: Uuid::new_v4(),
        username: body.username.clone(),
        email: body.email.to_lowercase(),
        password_hash: pw_hash,
        role: "user".to_string(),
        device_auth_pubkey: None,
        is_approved: !approval_required,
    };

    let max_users = user_service::resolved_max_users(&pool, &config)?;
    let user = user_service::create_user(&pool, new_user, max_users)?;
    if body.accepted_terms_version == Some(moderation_service::CURRENT_SAFETY_TERMS_VERSION) {
        let _ = moderation_service::record_terms_acceptance(
            &pool,
            user.id,
            moderation_service::CURRENT_SAFETY_TERMS_VERSION,
        )?;
    }
    record_registration(&client_ip);

    if approval_required {
        return Ok(HttpResponse::Accepted().json(serde_json::json!({
            "status": "pending",
            "message": "Your account is awaiting admin approval before you can log in."
        })));
    }
    Ok(HttpResponse::Created().json(UserPublic::from(user)))
}

/// GET /auth/me → 200 UserPublic
pub async fn me(pool: web::Data<Pool>, auth: AuthUser) -> Result<HttpResponse, AppError> {
    let user = user_service::find_by_id(&pool, auth.0.user_id()?)?.ok_or(AppError::Unauthorized)?;
    Ok(HttpResponse::Ok().json(UserPublic::from(user)))
}

/// GET /auth/setup-status → 200 { needs_setup: boolean }
pub async fn setup_status(pool: web::Data<Pool>) -> Result<HttpResponse, AppError> {
    let mut conn = pool.get()?;
    let admin_count: i64 = user_dsl::users
        .filter(user_dsl::role.eq("admin"))
        .count()
        .get_result(&mut conn)?;
    if admin_count > 0 {
        clear_setup_token();
    }

    Ok(HttpResponse::Ok().json(SetupStatusResponse {
        needs_setup: admin_count == 0,
    }))
}

/// POST /auth/setup-admin → 201 UserPublic
///
/// Bootstraps the very first admin account. Once any admin exists, this route
/// returns 409 and can no longer be used.
pub async fn setup_admin(
    pool: web::Data<Pool>,
    body: web::Json<SetupAdminRequest>,
) -> Result<HttpResponse, AppError> {
    let username = body.username.trim();
    let email = body.email.trim().to_lowercase();
    let password = body.password.as_str();
    let setup_token = body.setup_token.as_deref().unwrap_or("").trim();

    if username.is_empty() || email.is_empty() || password.len() < 8 {
        return Err(AppError::BadRequest(
            "username, email required; password must be ≥8 chars".into(),
        ));
    }
    if !is_valid_username(username) {
        return Err(AppError::BadRequest(
            "username must be 2-32 characters (no control characters)".into(),
        ));
    }

    let mut conn = pool.get()?;
    let admin_count: i64 = user_dsl::users
        .filter(user_dsl::role.eq("admin"))
        .count()
        .get_result(&mut conn)?;

    if admin_count > 0 {
        clear_setup_token();
        return Err(AppError::Conflict(
            "Admin account is already configured".into(),
        ));
    }
    if setup_token.is_empty() || !is_setup_token_valid(setup_token) {
        return Err(AppError::Forbidden);
    }

    let pw_hash = hash_password(password.to_string()).await?;
    let new_user = NewUser {
        id: Uuid::new_v4(),
        username: username.to_string(),
        email,
        password_hash: pw_hash,
        role: "admin".to_string(),
        device_auth_pubkey: None,
        is_approved: true,
    };

    let user = user_service::create_user(&pool, new_user, None)?;
    clear_setup_token();
    Ok(HttpResponse::Created().json(UserPublic::from(user)))
}

/// POST /auth/login → 200 TokenResponse
pub async fn login(
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, AppError> {
    let user = user_service::find_by_email(&pool, &body.email.to_lowercase())?
        .ok_or(AppError::Unauthorized)?;

    let pw_ok = verify_password(body.password.clone(), user.password_hash.clone()).await?;
    if !pw_ok {
        return Err(AppError::Unauthorized);
    }

    if !user.is_approved {
        return Err(AppError::Forbidden);
    }

    if let Some(hmac_key) = &config.altcha_hmac_key {
        let payload = body.altcha_payload.as_deref().unwrap_or("");
        if payload.is_empty() {
            return Err(AppError::BadRequest("altcha_payload is required".into()));
        }
        if !verify_altcha(payload, hmac_key) {
            return Err(AppError::Unauthorized);
        }
    }

    let mut conn = pool.get()?;
    if body.accepted_terms_version == Some(moderation_service::CURRENT_SAFETY_TERMS_VERSION) {
        moderation_service::record_terms_acceptance_conn(
            &mut conn,
            user.id,
            moderation_service::CURRENT_SAFETY_TERMS_VERSION,
        )?;
    }
    let tokens = mint_tokens(
        &mut conn,
        user.id,
        &user.role,
        Uuid::new_v4(), // new family for each login
        &config,
    )?;

    Ok(HttpResponse::Ok().json(tokens))
}

/// POST /auth/device-login/challenge → 200 DeviceLoginChallengeResponse
///
/// Creates a short-lived X25519 challenge. The server stores only the derived
/// proof key for one minute; the ephemeral private key is consumed immediately.
/// A caller must prove possession of the device's private X25519 key before the
/// login endpoint will issue tokens.
pub async fn create_device_login_challenge(
    redis: web::Data<redis::Client>,
    body: web::Json<DeviceLoginChallengeRequest>,
) -> Result<HttpResponse, AppError> {
    let pubkey = body.device_auth_pubkey.trim();
    let peer_public_bytes = decode_device_public_key(pubkey)?;

    let rng = SystemRandom::new();
    let server_private = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &rng)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("X25519 key generation failed")))?;
    let server_public = server_private
        .compute_public_key()
        .map_err(|_| AppError::Internal(anyhow::anyhow!("X25519 public key generation failed")))?;
    let peer_public = agreement::UnparsedPublicKey::new(&agreement::X25519, peer_public_bytes);
    let proof_key = agreement::agree_ephemeral(server_private, &peer_public, |key_material| {
        key_material.to_vec()
    })
    .map_err(|_| AppError::BadRequest("invalid X25519 device public key".into()))?;

    let mut challenge_bytes = [0u8; 32];
    rng.fill(&mut challenge_bytes)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("RNG failed")))?;
    let challenge_id = hex::encode(challenge_bytes);
    let record = DeviceLoginChallengeRecord {
        device_auth_pubkey: pubkey.to_string(),
        proof_key: BASE64_STANDARD.encode(proof_key),
    };
    let record_json = serde_json::to_string(&record)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("challenge serialization: {e}")))?;

    let mut conn = redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
    conn.set_ex::<_, _, ()>(
        format!("device_login_challenge:{challenge_id}"),
        record_json,
        DEVICE_LOGIN_CHALLENGE_TTL_SECS,
    )
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis set: {e}")))?;

    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(DeviceLoginChallengeResponse {
            challenge_id,
            server_ephemeral_pubkey: BASE64_STANDARD.encode(server_public.as_ref()),
            expires_in: DEVICE_LOGIN_CHALLENGE_TTL_SECS,
        }))
}

/// POST /auth/ws-ticket → a short-lived, single-use WebSocket ticket.
///
/// The reusable access token stays in the Authorization header and is never
/// placed in a URL. The ticket is consumed atomically by the WebSocket route.
pub async fn create_ws_ticket(
    redis: web::Data<redis::Client>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let rng = SystemRandom::new();
    let mut raw = [0u8; 32];
    rng.fill(&mut raw)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("RNG failed")))?;
    let ticket = hex::encode(raw);

    let mut conn = redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
    conn.set_ex::<_, _, ()>(
        format!("ws_ticket:{ticket}"),
        user_id.to_string(),
        WS_TICKET_TTL_SECS,
    )
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis set: {e}")))?;

    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(WsTicketResponse {
            ticket,
            expires_in: WS_TICKET_TTL_SECS,
        }))
}

async fn verify_device_login_proof(
    redis: &redis::Client,
    device_auth_pubkey: &str,
    challenge_id: &str,
    proof: &str,
) -> Result<(), AppError> {
    if challenge_id.len() != 64 || !challenge_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::Unauthorized);
    }

    let mut conn = redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
    // GETDEL makes each challenge single-use, including failed proof attempts.
    let record_json: Option<String> = conn
        .get_del(format!("device_login_challenge:{challenge_id}"))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis getdel: {e}")))?;
    let record_json = record_json.ok_or(AppError::Unauthorized)?;
    let record: DeviceLoginChallengeRecord =
        serde_json::from_str(&record_json).map_err(|_| AppError::Unauthorized)?;

    if record.device_auth_pubkey != device_auth_pubkey.trim() {
        return Err(AppError::Unauthorized);
    }
    let proof_key = BASE64_STANDARD
        .decode(record.proof_key)
        .map_err(|_| AppError::Unauthorized)?;
    let proof = BASE64_STANDARD
        .decode(proof.trim())
        .map_err(|_| AppError::Unauthorized)?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, &proof_key);
    hmac::verify(
        &key,
        device_login_proof_message(challenge_id, device_auth_pubkey).as_bytes(),
        &proof,
    )
    .map_err(|_| AppError::Unauthorized)
}

/// Decide how a device-login request may proceed, before any account lookup.
///
/// - `challenge_id` + `proof` together: verified strictly (single-use challenge).
/// - neither: the legacy body of installed apps <= 1.3.2. Accepted (and counted
///   and logged, never the key) unless `DEVICE_LOGIN_REQUIRE_CHALLENGE` is on.
/// - exactly one of them: always rejected, so a request can never be silently
///   downgraded to the legacy path.
async fn authorize_device_login(
    req: &HttpRequest,
    redis: &redis::Client,
    config: &Config,
    body: &DeviceLoginRequest,
) -> Result<(), AppError> {
    let pubkey = body.device_auth_pubkey.trim();
    match (body.challenge_id.as_deref(), body.proof.as_deref()) {
        (Some(challenge_id), Some(proof)) => {
            decode_device_public_key(pubkey)?;
            verify_device_login_proof(redis, pubkey, challenge_id, proof).await
        }
        (None, None) if !config.device_login_require_challenge => {
            if pubkey.is_empty() {
                return Err(AppError::BadRequest(
                    "device_auth_pubkey is required".into(),
                ));
            }
            legacy::record_device_login();
            tracing::warn!(
                user_agent = user_agent(req),
                "device_login_legacy: accepted device-login without a challenge; client should use /auth/device-login/challenge"
            );
            Ok(())
        }
        (None, None) => {
            tracing::warn!(
                user_agent = user_agent(req),
                "device_login_legacy_rejected: device-login without a challenge refused (DEVICE_LOGIN_REQUIRE_CHALLENGE is on)"
            );
            Err(AppError::BadRequest(
                "challenge_id and proof are required".into(),
            ))
        }
        _ => Err(AppError::BadRequest(
            "challenge_id and proof must be sent together".into(),
        )),
    }
}

fn user_agent(req: &HttpRequest) -> &str {
    req.headers()
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

/// POST /auth/device-login → 200 TokenResponse
///
/// Authenticates by a device-generated public key. If this key has never been
/// seen on this server, a new local user is created automatically.
pub async fn device_login(
    req: HttpRequest,
    pool: web::Data<Pool>,
    redis: web::Data<redis::Client>,
    config: web::Data<Config>,
    body: web::Json<DeviceLoginRequest>,
) -> Result<HttpResponse, AppError> {
    let pubkey = body.device_auth_pubkey.trim();
    authorize_device_login(&req, &redis, &config, &body).await?;
    // Extract client IP early; we'll apply the registration limiter only when
    // this is a *new* device (i.e. account creation, not re-login).
    let client_ip = extract_client_ip(&req);

    if let Some(hmac_key) = &config.altcha_hmac_key {
        let payload = body.altcha_payload.as_deref().unwrap_or("");
        if payload.is_empty() {
            return Err(AppError::BadRequest("altcha_payload is required".into()));
        }
        if !verify_altcha(payload, hmac_key) {
            return Err(AppError::Unauthorized);
        }
    }

    let user = if let Some(existing) = user_service::find_by_device_auth_pubkey(&pool, pubkey)? {
        if !existing.is_active {
            return Err(AppError::Unauthorized);
        }
        if !existing.is_approved {
            return Err(AppError::Forbidden);
        }
        existing
    } else {
        // New device → new account creation. Enforce the per-IP hourly limit.
        if is_registration_rate_limited(&client_ip) {
            return Err(AppError::TooManyRequests(
                "Too many accounts created from this IP. Try again later.".into(),
            ));
        }

        let pubkey_hash = hex::encode(digest(&SHA256, pubkey.as_bytes()).as_ref());
        let username = format!("u{}", &pubkey_hash[..15]);
        let shadow_email = format!("device+{}@device.sync.invalid", &pubkey_hash[..32]);
        let password_hash = hash_password(Uuid::new_v4().to_string()).await?;

        let approval_required = admin_service::is_approval_required(&pool)?;
        let new_user = NewUser {
            id: Uuid::new_v4(),
            username,
            email: shadow_email,
            password_hash,
            role: "user".to_string(),
            device_auth_pubkey: Some(pubkey.to_string()),
            is_approved: !approval_required,
        };

        let max_users = user_service::resolved_max_users(&pool, &config)?;
        match user_service::create_user(&pool, new_user, max_users) {
            Ok(created) => {
                record_registration(&client_ip);
                if approval_required {
                    return Ok(HttpResponse::Accepted().json(serde_json::json!({
                        "status": "pending",
                        "message": "Your account is awaiting admin approval before you can log in."
                    })));
                }
                created
            }
            Err(AppError::Conflict(_)) => {
                let existing = user_service::find_by_device_auth_pubkey(&pool, pubkey)?
                    .ok_or(AppError::Unauthorized)?;
                if !existing.is_active {
                    return Err(AppError::Unauthorized);
                }
                if !existing.is_approved {
                    return Err(AppError::Forbidden);
                }
                existing
            }
            Err(other) => return Err(other),
        }
    };

    let mut conn = pool.get()?;
    if body.accepted_terms_version == Some(moderation_service::CURRENT_SAFETY_TERMS_VERSION) {
        moderation_service::record_terms_acceptance_conn(
            &mut conn,
            user.id,
            moderation_service::CURRENT_SAFETY_TERMS_VERSION,
        )?;
    }
    let tokens = mint_tokens(
        &mut conn,
        user.id,
        &user.role,
        Uuid::new_v4(), // new family for each device-login
        &config,
    )?;

    Ok(HttpResponse::Ok().json(tokens))
}

/// POST /auth/qr-login/session → 201 QrLoginCreateResponse
pub async fn create_qr_login_session(
    redis: web::Data<redis::Client>,
) -> Result<HttpResponse, AppError> {
    let session_id = Uuid::new_v4().to_string();
    let mut secret_raw = [0u8; 16];
    let rng = ring::rand::SystemRandom::new();
    rng.fill(&mut secret_raw)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("RNG failed")))?;
    let secret = hex::encode(secret_raw);

    let record = QrLoginSessionRecord {
        secret: secret.clone(),
        status: QR_LOGIN_STATUS_PENDING.to_string(),
        access_token: None,
        refresh_token: None,
        expires_in: None,
    };

    save_qr_login_record(&redis, &session_id, &record, QR_LOGIN_TTL_SECS).await?;

    let qr_payload = json!({
        "type": "sync_qr_login",
        "session_id": session_id.clone(),
        "secret": secret,
    })
    .to_string();

    Ok(HttpResponse::Created().json(QrLoginCreateResponse {
        session_id,
        qr_payload,
        expires_in: QR_LOGIN_TTL_SECS,
    }))
}

/// Shared by the POST and legacy GET polls: `secret` has already been taken
/// from the body or the query string.
async fn qr_login_status(
    redis: &redis::Client,
    session_id: &str,
    secret: &str,
) -> Result<QrLoginStatusResponse, AppError> {
    let session_id = session_id.trim();
    if session_id.is_empty() || secret.trim().is_empty() {
        return Err(AppError::BadRequest(
            "session_id and secret are required".into(),
        ));
    }

    let Some(record) = load_qr_login_record(redis, session_id).await? else {
        return Ok(QrLoginStatusResponse {
            status: QR_LOGIN_STATUS_EXPIRED.to_string(),
            access_token: None,
            refresh_token: None,
            expires_in: None,
        });
    };

    if record.secret != secret {
        return Err(AppError::Unauthorized);
    }

    if record.status == QR_LOGIN_STATUS_APPROVED {
        delete_qr_login_record(redis, session_id).await?;
    }

    Ok(QrLoginStatusResponse {
        status: record.status,
        access_token: record.access_token,
        refresh_token: record.refresh_token,
        expires_in: record.expires_in,
    })
}

/// POST /auth/qr-login/session/{session_id} → 200 QrLoginStatusResponse
///
/// Body `{ "secret": ... }`, so the secret stays out of URLs and access logs.
pub async fn poll_qr_login_session(
    redis: web::Data<redis::Client>,
    path: web::Path<QrLoginSessionPath>,
    body: web::Json<QrLoginPollRequest>,
) -> Result<HttpResponse, AppError> {
    let status = qr_login_status(&redis, &path.session_id, &body.secret).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(status))
}

/// GET /auth/qr-login/session/{session_id}?secret=... → 200 QrLoginStatusResponse
///
/// Deprecated poll from installed apps <= 1.3.2; counted as `qr_poll_legacy`.
pub async fn poll_qr_login_session_legacy(
    redis: web::Data<redis::Client>,
    path: web::Path<QrLoginSessionPath>,
    query: web::Query<QrLoginSessionQuery>,
) -> Result<HttpResponse, AppError> {
    let status = qr_login_status(&redis, &path.session_id, &query.secret).await?;
    legacy::record_qr_poll();
    Ok(HttpResponse::Ok().json(status))
}

/// POST /auth/qr-login/approve → 200 { status: "approved" }
pub async fn approve_qr_login(
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    redis: web::Data<redis::Client>,
    auth: AuthUser,
    body: web::Json<QrLoginApproveRequest>,
) -> Result<HttpResponse, AppError> {
    let session_id = body.session_id.trim();
    let secret = body.secret.trim();
    if session_id.is_empty() || secret.is_empty() {
        return Err(AppError::BadRequest(
            "session_id and secret are required".into(),
        ));
    }

    let Some(mut record) = load_qr_login_record(&redis, session_id).await? else {
        return Err(AppError::BadRequest("QR login session expired".into()));
    };

    if record.secret != secret {
        return Err(AppError::Unauthorized);
    }
    if record.status != QR_LOGIN_STATUS_PENDING {
        return Ok(HttpResponse::Ok().json(json!({ "status": record.status })));
    }

    let mut conn = pool.get()?;
    let tokens = mint_tokens(
        &mut conn,
        auth.0.user_id()?,
        auth.0.role.as_str(),
        Uuid::new_v4(),
        &config,
    )?;

    record.status = QR_LOGIN_STATUS_APPROVED.to_string();
    record.access_token = Some(tokens.access_token);
    record.refresh_token = Some(tokens.refresh_token);
    record.expires_in = Some(tokens.expires_in);
    save_qr_login_record(&redis, session_id, &record, QR_LOGIN_TTL_SECS).await?;

    Ok(HttpResponse::Ok().json(json!({ "status": QR_LOGIN_STATUS_APPROVED })))
}

/// POST /auth/refresh → 200 TokenResponse (token rotation with family replay detection)
pub async fn refresh(
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    body: web::Json<RefreshRequest>,
) -> Result<HttpResponse, AppError> {
    let token_hash = hash_token(&body.refresh_token);
    let mut conn = pool.get()?;

    let stored: Option<RefreshToken> = rt_dsl::refresh_tokens
        .filter(rt_dsl::token_hash.eq(&token_hash))
        .first::<RefreshToken>(&mut conn)
        .optional()?;

    let stored = stored.ok_or(AppError::Unauthorized)?;

    if stored.revoked {
        // Family replay detected — revoke the entire family to protect the account.
        diesel::update(rt_dsl::refresh_tokens.filter(rt_dsl::family.eq(stored.family)))
            .set(rt_dsl::revoked.eq(true))
            .execute(&mut conn)?;
        return Err(AppError::Unauthorized);
    }

    if stored.expires_at < Utc::now() {
        return Err(AppError::Unauthorized);
    }

    // Revoke the consumed token
    diesel::update(rt_dsl::refresh_tokens.find(stored.id))
        .set(rt_dsl::revoked.eq(true))
        .execute(&mut conn)?;

    let user = user_service::find_by_id(&pool, stored.user_id)?.ok_or(AppError::Unauthorized)?;

    let tokens = mint_tokens(
        &mut conn,
        user.id,
        &user.role,
        stored.family, // same family — rotated, not forked
        &config,
    )?;

    Ok(HttpResponse::Ok().json(tokens))
}

/// POST /auth/logout → 204 (revokes the presented refresh token's entire family)
pub async fn logout(
    pool: web::Data<Pool>,
    body: web::Json<RefreshRequest>,
) -> Result<HttpResponse, AppError> {
    let token_hash = hash_token(&body.refresh_token);
    let mut conn = pool.get()?;

    let stored: Option<RefreshToken> = rt_dsl::refresh_tokens
        .filter(rt_dsl::token_hash.eq(&token_hash))
        .first::<RefreshToken>(&mut conn)
        .optional()?;

    if let Some(rt) = stored {
        diesel::update(rt_dsl::refresh_tokens.filter(rt_dsl::family.eq(rt.family)))
            .set(rt_dsl::revoked.eq(true))
            .execute(&mut conn)?;
    }
    // Silently succeed even if the token was not found (idempotent logout)

    Ok(HttpResponse::NoContent().finish())
}

// ── Forgot / reset password ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordForm {
    pub token: String,
    pub new_password: String,
}

/// POST /auth/forgot-password
///
/// Always returns 200 regardless of whether the email is registered (prevents
/// user enumeration). Generates a 32-byte random token, stores it in Redis
/// with a 15-minute TTL, then sends a reset link via Resend.
pub async fn forgot_password(
    pool: web::Data<Pool>,
    redis: web::Data<redis::Client>,
    config: web::Data<Config>,
    body: web::Json<ForgotPasswordRequest>,
) -> Result<HttpResponse, AppError> {
    const MSG: &str = "If that email is registered, a reset link was sent.";

    let email = body.email.trim().to_lowercase();

    // Look up user silently — return the generic message even if not found.
    let user = user_service::find_by_email(&pool, &email)?;
    let Some(user) = user else {
        return Ok(HttpResponse::Ok().json(serde_json::json!({ "message": MSG })));
    };

    // Generate a 32-byte random token → hex string (64 chars).
    let rng = ring::rand::SystemRandom::new();
    let mut raw = [0u8; 32];
    rng.fill(&mut raw)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("RNG failed")))?;
    let token = hex::encode(raw);

    // Store reset:{token} → user_id in Redis with 15-minute TTL.
    let redis_key = format!("reset:{}", token);
    let mut conn = redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {}", e)))?;
    conn.set_ex::<_, _, ()>(&redis_key, user.id.to_string(), 900)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis set: {}", e)))?;

    // Build the reset URL pointing to this server's GET handler.
    let reset_url = format!(
        "http{}://{}/auth/reset-password?token={}",
        if config.enforce_https { "s" } else { "" },
        config.instance_domain,
        token
    );

    // Send email (silently skipped when RESEND_API_KEY is not set).
    email_service::send_password_reset(
        config.resend_api_key.as_deref(),
        &config.resend_from_email,
        &email,
        &reset_url,
    )
    .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "message": MSG })))
}

/// GET /auth/reset-password?token=...
///
/// Returns a minimal server-rendered HTML form the user fills in their browser.
pub async fn reset_password_form(
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    let token = query.get("token").cloned().unwrap_or_default();

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Reset your password – Sync</title>
  <style>
    body{{font-family:system-ui,sans-serif;max-width:400px;margin:60px auto;padding:0 20px;color:#111}}
    h1{{font-size:1.4rem;margin-bottom:4px}}
    p{{color:#555;margin-top:4px;font-size:.9rem}}
    label{{display:block;margin:16px 0 4px;font-size:.9rem;font-weight:600}}
    input{{width:100%;box-sizing:border-box;padding:10px 12px;border:1px solid #ccc;border-radius:8px;font-size:1rem}}
    button{{margin-top:20px;width:100%;padding:12px;background:#4F46E5;color:#fff;border:none;border-radius:8px;font-size:1rem;font-weight:600;cursor:pointer}}
    button:hover{{background:#4338CA}}
    .err{{color:#dc2626;font-size:.875rem;margin-top:8px}}
  </style>
</head>
<body>
  <h1>Reset your password</h1>
  <p>Enter a new password for your Sync account.</p>
  <form method="POST" action="/auth/reset-password">
    <input type="hidden" name="token" value="{token}">
    <label for="pw">New password (min 8 characters)</label>
    <input id="pw" name="new_password" type="password" required minlength="8" autocomplete="new-password">
    <label for="pw2">Confirm password</label>
    <input id="pw2" name="confirm" type="password" required minlength="8" autocomplete="new-password">
    <button type="submit">Set new password</button>
  </form>
  <script>
    document.querySelector('form').addEventListener('submit',function(e){{
      var pw=document.getElementById('pw').value;
      var c=document.getElementById('pw2').value;
      if(pw!==c){{e.preventDefault();var d=document.querySelector('.err');if(!d){{d=document.createElement('p');d.className='err';this.appendChild(d);}}d.textContent='Passwords do not match.';}}
    }}.bind(document.querySelector('form')));
  </script>
</body>
</html>"#,
        token = token
    );

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

/// POST /auth/reset-password
///
/// Accepts `application/x-www-form-urlencoded` (from the browser form) or
/// `application/json`. Validates the token, updates the password, deletes
/// the Redis key (single-use), and returns an HTML success page or JSON.
pub async fn reset_password(
    pool: web::Data<Pool>,
    redis: web::Data<redis::Client>,
    form: web::Form<ResetPasswordForm>,
) -> Result<HttpResponse, AppError> {
    let token = form.token.trim();
    let new_password = form.new_password.trim();

    if new_password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".into(),
        ));
    }

    // Look up user_id from Redis.
    let redis_key = format!("reset:{}", token);
    let mut conn = redis
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {}", e)))?;

    let user_id_str: Option<String> = conn
        .get(&redis_key)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis get: {}", e)))?;

    let user_id_str =
        user_id_str.ok_or_else(|| AppError::BadRequest("Invalid or expired token".into()))?;

    let user_id = Uuid::parse_str(&user_id_str)
        .map_err(|_| AppError::BadRequest("Invalid or expired token".into()))?;

    // Hash new password and update DB.
    let pw_hash = hash_password(new_password.to_string()).await?;
    let mut db_conn = pool.get()?;
    diesel::update(crate::schema::users::table.find(user_id))
        .set(crate::schema::users::password_hash.eq(&pw_hash))
        .execute(&mut db_conn)?;

    // Delete the Redis key (single-use token).
    conn.del::<_, ()>(&redis_key)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis del: {}", e)))?;

    tracing::info!(%user_id, "Password reset completed");

    let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Password updated – Sync</title>
  <style>
    body{font-family:system-ui,sans-serif;max-width:400px;margin:60px auto;padding:0 20px;color:#111;text-align:center}
    .icon{font-size:3rem;margin-bottom:8px}
    h1{font-size:1.4rem}
    p{color:#555;font-size:.9rem}
  </style>
</head>
<body>
  <div class="icon">✅</div>
  <h1>Password updated</h1>
  <p>Your password has been changed. You can now sign in to the Sync mobile app with your new password.</p>
</body>
</html>"#;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html))
}

/// Register all auth routes under a given service config.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/setup-status", web::get().to(setup_status))
        .route("/setup-admin", web::post().to(setup_admin))
        .route("/register", web::post().to(register))
        .route("/login", web::post().to(login))
        .route(
            "/device-login/challenge",
            web::post().to(create_device_login_challenge),
        )
        .route("/device-login", web::post().to(device_login))
        .route("/ws-ticket", web::post().to(create_ws_ticket))
        .route("/qr-login/session", web::post().to(create_qr_login_session))
        .route(
            "/qr-login/session/{session_id}",
            web::post().to(poll_qr_login_session),
        )
        .route(
            "/qr-login/session/{session_id}",
            web::get().to(poll_qr_login_session_legacy),
        )
        .route("/qr-login/approve", web::post().to(approve_qr_login))
        .route("/me", web::get().to(me))
        .route("/refresh", web::post().to(refresh))
        .route("/logout", web::post().to(logout))
        .route("/forgot-password", web::post().to(forgot_password))
        .route("/reset-password", web::get().to(reset_password_form))
        .route("/reset-password", web::post().to(reset_password));
}

#[cfg(test)]
mod tests {
    use actix_web::{http::StatusCode, test as actix_test, App};
    use diesel::r2d2::{self, ConnectionManager};
    use serde_json::Value;
    use tracing_actix_web::TracingLogger;

    use super::*;
    use crate::request_logging::test_support::LogBuffer;
    use crate::request_logging::PathOnlyRootSpanBuilder;

    #[test]
    fn username_allows_two_characters_and_trims() {
        assert!(is_valid_username("ab"));
        assert!(is_valid_username(" ab "));
    }

    #[test]
    fn username_rejects_too_short_values() {
        assert!(!is_valid_username("a"));
        assert!(!is_valid_username(" "));
    }

    #[test]
    fn username_rejects_control_characters() {
        assert!(!is_valid_username("hello\nworld"));
        assert!(!is_valid_username("name\u{0}test"));
    }

    // ── device-login challenge / QR poll ─────────────────────────────────────

    /// Nothing listens here, so a test that reaches Redis or Postgres by
    /// accident fails (or, for QR, uses the in-memory fallback on purpose).
    const DEAD_REDIS: &str = "redis://127.0.0.1:1";

    /// Redis for the tests that need a real one; set `TEST_REDIS_URL` to run them.
    fn live_redis() -> Option<(String, redis::Client)> {
        let url = std::env::var("TEST_REDIS_URL").ok()?;
        let client = redis::Client::open(url.as_str()).expect("TEST_REDIS_URL");
        Some((url, client))
    }

    /// One request through the real `/auth` routes. The database is deliberately
    /// unreachable: a request that gets past authentication surfaces as a 500.
    async fn send(
        redis_url: &str,
        require_challenge: bool,
        req: actix_test::TestRequest,
    ) -> (StatusCode, Value) {
        let pool: Pool = r2d2::Pool::builder()
            .connection_timeout(std::time::Duration::from_millis(200))
            .build_unchecked(ConnectionManager::new(
                "postgres://unused@127.0.0.1:1/unused",
            ));
        let config = Config {
            device_login_require_challenge: require_challenge,
            ..Config::for_tests()
        };
        let app = actix_test::init_service(
            App::new()
                .wrap(TracingLogger::<PathOnlyRootSpanBuilder>::new())
                .app_data(web::Data::new(config))
                .app_data(web::Data::new(pool))
                .app_data(web::Data::new(redis::Client::open(redis_url).unwrap()))
                .service(web::scope("/auth").configure(configure)),
        )
        .await;
        let resp = actix_test::call_service(&app, req.to_request()).await;
        let status = resp.status();
        let body = actix_test::read_body(resp).await;
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    /// The device side of the protocol, as the mobile client implements it.
    struct TestDevice {
        private: agreement::EphemeralPrivateKey,
        public_b64: String,
    }

    impl TestDevice {
        fn new() -> Self {
            let private =
                agreement::EphemeralPrivateKey::generate(&agreement::X25519, &SystemRandom::new())
                    .unwrap();
            let public_b64 = BASE64_STANDARD.encode(private.compute_public_key().unwrap());
            Self {
                private,
                public_b64,
            }
        }

        /// proof = base64(HMAC-SHA256(X25519(device_priv, server_pub),
        ///                            "device-login:<challenge_id>:<device_pub_b64>"))
        fn proof(self, challenge_id: &str, server_pubkey_b64: &str) -> String {
            let server_public = BASE64_STANDARD.decode(server_pubkey_b64).unwrap();
            let shared = agreement::agree_ephemeral(
                self.private,
                &agreement::UnparsedPublicKey::new(&agreement::X25519, server_public),
                |material| material.to_vec(),
            )
            .unwrap();
            let tag = hmac::sign(
                &hmac::Key::new(hmac::HMAC_SHA256, &shared),
                format!("device-login:{challenge_id}:{}", self.public_b64).as_bytes(),
            );
            BASE64_STANDARD.encode(tag.as_ref())
        }
    }

    async fn request_challenge(redis_url: &str, device_pubkey: &str) -> (StatusCode, Value) {
        send(
            redis_url,
            false,
            actix_test::TestRequest::post()
                .uri("/auth/device-login/challenge")
                .set_json(json!({ "device_auth_pubkey": device_pubkey })),
        )
        .await
    }

    fn login_request(body: Value) -> actix_test::TestRequest {
        actix_test::TestRequest::post()
            .uri("/auth/device-login")
            .insert_header(("User-Agent", "Sync/1.3.2 (test)"))
            .set_json(body)
    }

    /// Mints a challenge for `device` and returns `(challenge_id, proof)`.
    async fn challenge_and_proof(redis_url: &str, device: TestDevice) -> (String, String) {
        let (status, challenge) = request_challenge(redis_url, &device.public_b64).await;
        assert_eq!(status, StatusCode::OK, "{challenge}");
        let challenge_id = challenge["challenge_id"].as_str().unwrap().to_string();
        let proof = device.proof(
            &challenge_id,
            challenge["server_ephemeral_pubkey"].as_str().unwrap(),
        );
        (challenge_id, proof)
    }

    #[actix_web::test]
    async fn challenge_response_has_the_documented_shape() {
        let Some((url, redis)) = live_redis() else {
            eprintln!("skipped: TEST_REDIS_URL not set");
            return;
        };
        let device = TestDevice::new();
        let (status, body) = request_challenge(&url, &device.public_b64).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["expires_in"], 60);
        assert_eq!(body["challenge_id"].as_str().unwrap().len(), 64);
        let server_pub = BASE64_STANDARD
            .decode(body["server_ephemeral_pubkey"].as_str().unwrap())
            .unwrap();
        assert_eq!(server_pub.len(), 32);
        let mut keys = body
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(
            keys,
            ["challenge_id", "expires_in", "server_ephemeral_pubkey"]
        );
    }

    #[actix_web::test]
    async fn challenge_request_rejects_malformed_device_keys() {
        for bad in [
            "",
            "not base64!!",
            "AAAA",
            &BASE64_STANDARD.encode([7u8; 31]),
        ] {
            let (status, _) = request_challenge(DEAD_REDIS, bad).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{bad:?}");
        }
    }

    #[actix_web::test]
    async fn valid_proof_verifies_exactly_once() {
        let Some((url, redis)) = live_redis() else {
            eprintln!("skipped: TEST_REDIS_URL not set");
            return;
        };
        let device = TestDevice::new();
        let pubkey = device.public_b64.clone();
        let (challenge_id, proof) = challenge_and_proof(&url, device).await;

        verify_device_login_proof(&redis, &pubkey, &challenge_id, &proof)
            .await
            .expect("first use of a valid proof");
        assert!(
            matches!(
                verify_device_login_proof(&redis, &pubkey, &challenge_id, &proof).await,
                Err(AppError::Unauthorized)
            ),
            "a replayed challenge must be rejected"
        );
    }

    #[actix_web::test]
    async fn failed_proof_attempt_burns_the_challenge() {
        let Some((url, redis)) = live_redis() else {
            eprintln!("skipped: TEST_REDIS_URL not set");
            return;
        };
        let device = TestDevice::new();
        let pubkey = device.public_b64.clone();
        let (challenge_id, proof) = challenge_and_proof(&url, device).await;

        let wrong = BASE64_STANDARD.encode([0u8; 32]);
        assert!(matches!(
            verify_device_login_proof(&redis, &pubkey, &challenge_id, &wrong).await,
            Err(AppError::Unauthorized)
        ));
        assert!(
            matches!(
                verify_device_login_proof(&redis, &pubkey, &challenge_id, &proof).await,
                Err(AppError::Unauthorized)
            ),
            "the right proof must not work after a failed attempt"
        );
    }

    #[actix_web::test]
    async fn proof_is_bound_to_the_key_the_challenge_was_issued_for() {
        let Some((url, redis)) = live_redis() else {
            eprintln!("skipped: TEST_REDIS_URL not set");
            return;
        };
        let (challenge_id, proof) = challenge_and_proof(&url, TestDevice::new()).await;
        let other = TestDevice::new().public_b64;
        assert!(matches!(
            verify_device_login_proof(&redis, &other, &challenge_id, &proof).await,
            Err(AppError::Unauthorized)
        ));
    }

    #[actix_web::test]
    async fn challenge_expires_after_its_ttl() {
        let Some((url, redis)) = live_redis() else {
            eprintln!("skipped: TEST_REDIS_URL not set");
            return;
        };
        let device = TestDevice::new();
        let pubkey = device.public_b64.clone();
        let (challenge_id, proof) = challenge_and_proof(&url, device).await;

        // The key carries a TTL of at most 60s; deleting it is what Redis does
        // when that TTL runs out.
        let mut conn = redis.get_multiplexed_async_connection().await.unwrap();
        let key = format!("device_login_challenge:{challenge_id}");
        let ttl: i64 = redis::cmd("TTL")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!((1..=60).contains(&ttl), "ttl was {ttl}");
        let _: () = conn.del(&key).await.unwrap();

        assert!(matches!(
            verify_device_login_proof(&redis, &pubkey, &challenge_id, &proof).await,
            Err(AppError::Unauthorized)
        ));
    }

    #[actix_web::test]
    async fn malformed_challenge_ids_are_unauthorized_before_redis_is_touched() {
        let redis = redis::Client::open(DEAD_REDIS).unwrap();
        for bad in [
            "",
            "short",
            &"g".repeat(64),
            &"a".repeat(63),
            &"a".repeat(65),
        ] {
            assert!(
                matches!(
                    verify_device_login_proof(&redis, "key", bad, "proof").await,
                    Err(AppError::Unauthorized)
                ),
                "{bad:?}"
            );
        }
    }

    #[actix_web::test]
    async fn device_login_with_both_fields_never_falls_back_to_the_legacy_path() {
        let device = TestDevice::new();
        let (status, _) = send(
            DEAD_REDIS,
            false,
            login_request(json!({
                "device_auth_pubkey": device.public_b64,
                "challenge_id": "ab".repeat(32),
                "proof": BASE64_STANDARD.encode([1u8; 32]),
            })),
        )
        .await;
        // Redis is unreachable here, so this is an internal error rather than a
        // silent fall-through to the legacy path; the live-Redis cases above and
        // the end-to-end run cover the 401.
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[actix_web::test]
    async fn device_login_with_only_one_of_challenge_id_and_proof_is_rejected() {
        let pubkey = TestDevice::new().public_b64;
        for require_challenge in [false, true] {
            for body in [
                json!({ "device_auth_pubkey": pubkey, "challenge_id": "ab".repeat(32) }),
                json!({ "device_auth_pubkey": pubkey, "proof": "AAAA" }),
            ] {
                let (status, _) = send(DEAD_REDIS, require_challenge, login_request(body)).await;
                assert_eq!(
                    status,
                    StatusCode::BAD_REQUEST,
                    "require_challenge={require_challenge}"
                );
            }
        }
    }

    #[actix_web::test]
    async fn legacy_device_login_is_counted_when_allowed_and_refused_when_required() {
        let logs = LogBuffer::default();
        let _guard = logs.capture();
        let pubkey = TestDevice::new().public_b64;
        let body = || json!({ "device_auth_pubkey": pubkey, "accepted_terms_version": 1 });

        // Flag on: refused like a body that is missing required fields, and not counted.
        let before = legacy::device_login_count();
        let (status, _) = send(DEAD_REDIS, true, login_request(body())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(legacy::device_login_count(), before);

        // Flag off (default): the legacy body passes authentication. The database
        // is unreachable, so the request then fails there with a 500, which is how
        // this test tells "accepted" from "rejected" (400/401).
        let (status, _) = send(DEAD_REDIS, false, login_request(body())).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(legacy::device_login_count(), before + 1);

        let out = logs.contents();
        assert!(out.contains("device_login_legacy:"), "no legacy log: {out}");
        assert!(
            out.contains("device_login_legacy_rejected"),
            "no rejection log: {out}"
        );
        assert!(
            out.contains("Sync/1.3.2 (test)"),
            "user agent missing: {out}"
        );
        assert!(
            !out.contains(&pubkey),
            "device public key leaked into logs: {out}"
        );
    }

    #[actix_web::test]
    async fn legacy_device_login_still_requires_a_key() {
        let (status, body) = send(
            DEAD_REDIS,
            false,
            login_request(json!({ "device_auth_pubkey": "  " })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "Bad request: device_auth_pubkey is required");
    }

    #[actix_web::test]
    async fn challenge_login_rejects_a_malformed_device_key_before_touching_redis() {
        let (status, _) = send(
            DEAD_REDIS,
            false,
            login_request(json!({
                "device_auth_pubkey": "AAAA",
                "challenge_id": "ab".repeat(32),
                "proof": "AAAA",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn qr_session_can_be_polled_with_post_and_with_the_legacy_get() {
        let logs = LogBuffer::default();
        let _guard = logs.capture();
        // Redis is unreachable on purpose: QR sessions use the in-process store
        // then, which keeps this test independent of any service.
        let (status, created) = send(
            DEAD_REDIS,
            false,
            actix_test::TestRequest::post().uri("/auth/qr-login/session"),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let session_id = created["session_id"].as_str().unwrap().to_string();
        let payload: Value = serde_json::from_str(created["qr_payload"].as_str().unwrap()).unwrap();
        let secret = payload["secret"].as_str().unwrap().to_string();
        let uri = format!("/auth/qr-login/session/{session_id}");

        let before = legacy::qr_poll_count();
        let (status, body) = send(
            DEAD_REDIS,
            false,
            actix_test::TestRequest::post()
                .uri(&uri)
                .set_json(json!({ "secret": secret })),
        )
        .await;
        assert_eq!(
            (status, body["status"].as_str()),
            (StatusCode::OK, Some("pending"))
        );
        assert_eq!(legacy::qr_poll_count(), before, "POST is not a legacy poll");

        let (status, body) = send(
            DEAD_REDIS,
            false,
            actix_test::TestRequest::get().uri(&format!("{uri}?secret={secret}")),
        )
        .await;
        assert_eq!(
            (status, body["status"].as_str()),
            (StatusCode::OK, Some("pending"))
        );
        assert_eq!(legacy::qr_poll_count(), before + 1, "GET is counted");

        // A wrong secret is refused on both methods and does not count.
        let (status, _) = send(
            DEAD_REDIS,
            false,
            actix_test::TestRequest::post()
                .uri(&uri)
                .set_json(json!({ "secret": "wrong" })),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = send(
            DEAD_REDIS,
            false,
            actix_test::TestRequest::get().uri(&format!("{uri}?secret=wrong")),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(legacy::qr_poll_count(), before + 1);

        let out = logs.contents();
        assert!(!out.contains(&secret), "QR secret leaked into logs: {out}");
        assert!(
            !out.contains("?secret"),
            "query string leaked into logs: {out}"
        );
    }

    #[actix_web::test]
    async fn qr_poll_without_a_secret_is_a_bad_request() {
        let (status, _) = send(
            DEAD_REDIS,
            false,
            actix_test::TestRequest::post()
                .uri("/auth/qr-login/session/some-session")
                .set_json(json!({ "secret": " " })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
