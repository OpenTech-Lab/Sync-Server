use actix_web::{web, FromRequest, HttpRequest};
use futures_util::future::LocalBoxFuture;

use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{agent_token_service, user_service};

use super::claims::Claims;
use super::tokens::verify_access_token;

/// Extractor for any authenticated user.
pub struct AuthUser(pub Claims);

/// Extractor for admin-only routes. Fails with 403 if role != "admin".
#[allow(dead_code)]
pub struct AdminUser(pub Claims);

fn bearer_token(req: &HttpRequest) -> Result<&str, AppError> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    auth_header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)
}

fn extract_claims(req: &HttpRequest) -> Result<Claims, AppError> {
    let config = req
        .app_data::<web::Data<Config>>()
        .ok_or(AppError::Unauthorized)?;

    let token = bearer_token(req)?;
    verify_access_token(token, &config.jwt_secret).map_err(|_| AppError::Unauthorized)
}

/// Result of authenticating an admin-route request: either a normal JWT
/// (still subject to the existing active-user + role DB check below) or an
/// agent token (already fully validated — active, non-revoked, non-expired
/// — by `agent_token_service::verify`, so it's trusted directly).
enum AdminAuth {
    Jwt(Claims),
    AgentToken(Claims),
}

/// Same as `extract_claims`, but for admin-only routes: if the bearer string
/// isn't a valid JWT and starts with `agt_`, it's checked against the
/// `agent_tokens` table instead.
async fn extract_admin_auth(req: &HttpRequest, pool: &Pool) -> Result<AdminAuth, AppError> {
    let config = req
        .app_data::<web::Data<Config>>()
        .ok_or(AppError::Unauthorized)?;
    let token = bearer_token(req)?;

    if let Ok(claims) = verify_access_token(token, &config.jwt_secret) {
        return Ok(AdminAuth::Jwt(claims));
    }

    if !token.starts_with("agt_") {
        return Err(AppError::Unauthorized);
    }

    let agent_token = agent_token_service::verify(pool, token)?.ok_or(AppError::Unauthorized)?;

    // A token loses power the moment its creator does: require the creator
    // to still be an active admin, rather than trusting the token forever.
    let creator = user_service::find_by_id(pool, agent_token.created_by)?;
    let creator = match creator {
        Some(u) if u.is_active && u.role == "admin" => u,
        _ => return Err(AppError::Unauthorized),
    };

    let now = chrono::Utc::now().timestamp();
    Ok(AdminAuth::AgentToken(Claims::new(
        agent_token.created_by,
        creator.role,
        now,
        now + 300,
    )))
}

impl FromRequest for AuthUser {
    type Error = AppError;
    type Future = LocalBoxFuture<'static, Result<AuthUser, AppError>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let claims = extract_claims(req);
        let pool = req.app_data::<web::Data<Pool>>().cloned();
        Box::pin(async move {
            let claims = claims?;
            let user_id = claims.user_id().map_err(|_| AppError::Unauthorized)?;
            let pool = pool.ok_or(AppError::Unauthorized)?;
            let user = user_service::find_by_id(&pool, user_id)?;
            match user {
                Some(u) if u.is_active => Ok(AuthUser(claims)),
                _ => Err(AppError::Unauthorized),
            }
        })
    }
}

impl FromRequest for AdminUser {
    type Error = AppError;
    type Future = LocalBoxFuture<'static, Result<AdminUser, AppError>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let req = req.clone();
        let pool = req.app_data::<web::Data<Pool>>().cloned();
        Box::pin(async move {
            let pool = pool.ok_or(AppError::Unauthorized)?;
            let auth = extract_admin_auth(&req, &pool).await?;

            // Agent-token claims are already fully validated by
            // `extract_admin_auth` (active, non-revoked, non-expired) —
            // trusted directly, no further DB check needed.
            let claims = match auth {
                AdminAuth::AgentToken(claims) => return Ok(AdminUser(claims)),
                AdminAuth::Jwt(claims) => claims,
            };

            // JWT path: unchanged from the pre-existing behavior — still
            // requires the user row to exist and be active, and role == "admin".
            let user_id = claims.user_id().map_err(|_| AppError::Unauthorized)?;
            let user = user_service::find_by_id(&pool, user_id)?;
            match user {
                Some(u) if u.is_active => {
                    if claims.role == "admin" {
                        Ok(AdminUser(claims))
                    } else {
                        Err(AppError::Forbidden)
                    }
                }
                _ => Err(AppError::Unauthorized),
            }
        })
    }
}
