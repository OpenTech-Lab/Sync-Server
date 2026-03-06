use actix_web::{web, HttpResponse};
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use chrono::Utc;
use diesel::prelude::*;
use redis::AsyncCommands;
use ring::digest::{digest, SHA256};
use ring::rand::SecureRandom;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

use crate::auth::claims::Claims;
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
use crate::services::{email_service, user_service};

// ── Request / Response DTOs ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub altcha_payload: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub altcha_payload: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeviceLoginRequest {
    pub device_auth_pubkey: String,
    pub altcha_payload: Option<String>,
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

#[derive(Debug, Deserialize)]
pub struct QrLoginSessionQuery {
    pub secret: String,
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
const QR_LOGIN_STATUS_PENDING: &str = "pending";
const QR_LOGIN_STATUS_APPROVED: &str = "approved";
const QR_LOGIN_STATUS_EXPIRED: &str = "expired";

static QR_LOGIN_FALLBACK_STORE: OnceLock<Mutex<HashMap<String, (QrLoginSessionRecord, i64)>>> =
    OnceLock::new();
static ADMIN_SETUP_TOKEN: OnceLock<Mutex<Option<String>>> = OnceLock::new();

// ── Helpers ──────────────────────────────────────────────────────────────────

fn is_valid_username(value: &str) -> bool {
    let normalized = value.trim();
    let len = normalized.chars().count();
    if !(3..=32).contains(&len) {
        return false;
    }
    normalized
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-')
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
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    body: web::Json<RegisterRequest>,
) -> Result<HttpResponse, AppError> {
    // Validate minimal input
    if body.username.trim().is_empty() || body.email.trim().is_empty() || body.password.len() < 8 {
        return Err(AppError::BadRequest(
            "username, email required; password must be ≥8 chars".into(),
        ));
    }
    if !is_valid_username(&body.username) {
        return Err(AppError::BadRequest(
            "username must be 3-32 chars and only contain a-zA-Z0-9._-".into(),
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

    let new_user = NewUser {
        id: Uuid::new_v4(),
        username: body.username.clone(),
        email: body.email.to_lowercase(),
        password_hash: pw_hash,
        role: "user".to_string(),
        device_auth_pubkey: None,
    };

    let max_users = user_service::resolved_max_users(&pool, &config)?;
    let user = user_service::create_user(&pool, new_user, max_users)?;
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
            "username must be 3-32 chars and only contain a-zA-Z0-9._-".into(),
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
    let tokens = mint_tokens(
        &mut conn,
        user.id,
        &user.role,
        Uuid::new_v4(), // new family for each login
        &config,
    )?;

    Ok(HttpResponse::Ok().json(tokens))
}

/// POST /auth/device-login → 200 TokenResponse
///
/// Authenticates by a device-generated public key. If this key has never been
/// seen on this server, a new local user is created automatically.
pub async fn device_login(
    pool: web::Data<Pool>,
    config: web::Data<Config>,
    body: web::Json<DeviceLoginRequest>,
) -> Result<HttpResponse, AppError> {
    let pubkey = body.device_auth_pubkey.trim();
    if pubkey.is_empty() {
        return Err(AppError::BadRequest(
            "device_auth_pubkey is required".into(),
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

    let user = if let Some(existing) = user_service::find_by_device_auth_pubkey(&pool, pubkey)? {
        if !existing.is_active {
            return Err(AppError::Unauthorized);
        }
        existing
    } else {
        let pubkey_hash = hex::encode(digest(&SHA256, pubkey.as_bytes()).as_ref());
        let username = format!("u{}", &pubkey_hash[..15]);
        let shadow_email = format!("device+{}@device.sync.invalid", &pubkey_hash[..32]);
        let password_hash = hash_password(Uuid::new_v4().to_string()).await?;

        let new_user = NewUser {
            id: Uuid::new_v4(),
            username,
            email: shadow_email,
            password_hash,
            role: "user".to_string(),
            device_auth_pubkey: Some(pubkey.to_string()),
        };

        let max_users = user_service::resolved_max_users(&pool, &config)?;
        match user_service::create_user(&pool, new_user, max_users) {
            Ok(created) => created,
            Err(AppError::Conflict(_)) => {
                let existing = user_service::find_by_device_auth_pubkey(&pool, pubkey)?
                    .ok_or(AppError::Unauthorized)?;
                if !existing.is_active {
                    return Err(AppError::Unauthorized);
                }
                existing
            }
            Err(other) => return Err(other),
        }
    };

    let mut conn = pool.get()?;
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

/// GET /auth/qr-login/session/{session_id}?secret=... → 200 QrLoginStatusResponse
pub async fn poll_qr_login_session(
    redis: web::Data<redis::Client>,
    path: web::Path<QrLoginSessionPath>,
    query: web::Query<QrLoginSessionQuery>,
) -> Result<HttpResponse, AppError> {
    let session_id = path.session_id.trim();
    if session_id.is_empty() || query.secret.trim().is_empty() {
        return Err(AppError::BadRequest(
            "session_id and secret are required".into(),
        ));
    }

    let Some(record) = load_qr_login_record(&redis, session_id).await? else {
        return Ok(HttpResponse::Ok().json(QrLoginStatusResponse {
            status: QR_LOGIN_STATUS_EXPIRED.to_string(),
            access_token: None,
            refresh_token: None,
            expires_in: None,
        }));
    };

    if record.secret != query.secret {
        return Err(AppError::Unauthorized);
    }

    if record.status == QR_LOGIN_STATUS_APPROVED {
        delete_qr_login_record(&redis, session_id).await?;
    }

    Ok(HttpResponse::Ok().json(QrLoginStatusResponse {
        status: record.status,
        access_token: record.access_token,
        refresh_token: record.refresh_token,
        expires_in: record.expires_in,
    }))
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
        .route("/device-login", web::post().to(device_login))
        .route("/qr-login/session", web::post().to(create_qr_login_session))
        .route(
            "/qr-login/session/{session_id}",
            web::get().to(poll_qr_login_session),
        )
        .route("/qr-login/approve", web::post().to(approve_qr_login))
        .route("/me", web::get().to(me))
        .route("/refresh", web::post().to(refresh))
        .route("/logout", web::post().to(logout))
        .route("/forgot-password", web::post().to(forgot_password))
        .route("/reset-password", web::get().to(reset_password_form))
        .route("/reset-password", web::post().to(reset_password));
}
