use actix_web::{web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
use redis::AsyncCommands;
use ring::rand::SecureRandom;
use serde::{Deserialize, Serialize};
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
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
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

// ── Helpers ──────────────────────────────────────────────────────────────────

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

    let pw_hash = hash_password(body.password.clone()).await?;

    let new_user = NewUser {
        id: Uuid::new_v4(),
        username: body.username.clone(),
        email: body.email.to_lowercase(),
        password_hash: pw_hash,
        role: "user".to_string(),
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

    if username.is_empty() || email.is_empty() || password.len() < 8 {
        return Err(AppError::BadRequest(
            "username, email required; password must be ≥8 chars".into(),
        ));
    }

    let mut conn = pool.get()?;
    let admin_count: i64 = user_dsl::users
        .filter(user_dsl::role.eq("admin"))
        .count()
        .get_result(&mut conn)?;

    if admin_count > 0 {
        return Err(AppError::Conflict("Admin account is already configured".into()));
    }

    let pw_hash = hash_password(password.to_string()).await?;
    let new_user = NewUser {
        id: Uuid::new_v4(),
        username: username.to_string(),
        email,
        password_hash: pw_hash,
        role: "admin".to_string(),
    };

    let user = user_service::create_user(&pool, new_user, None)?;
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
        .route("/me", web::get().to(me))
        .route("/refresh", web::post().to(refresh))
        .route("/logout", web::post().to(logout))
        .route("/forgot-password", web::post().to(forgot_password))
        .route("/reset-password", web::get().to(reset_password_form))
        .route("/reset-password", web::post().to(reset_password));
}
