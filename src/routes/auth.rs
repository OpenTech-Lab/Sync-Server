use actix_web::{web, HttpResponse};
use chrono::Utc;
use diesel::prelude::*;
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
use crate::services::user_service;

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

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    /// Access token lifetime in seconds.
    pub expires_in: u64,
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

/// Register all auth routes under a given service config.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/register", web::post().to(register))
        .route("/login", web::post().to(login))
        .route("/me", web::get().to(me))
        .route("/refresh", web::post().to(refresh))
        .route("/logout", web::post().to(logout));
}
