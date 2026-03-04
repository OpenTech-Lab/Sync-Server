use actix_web::{web, FromRequest, HttpRequest};
use futures_util::future::LocalBoxFuture;

use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::user_service;

use super::claims::Claims;
use super::tokens::verify_access_token;

/// Extractor for any authenticated user.
pub struct AuthUser(pub Claims);

/// Extractor for admin-only routes. Fails with 403 if role != "admin".
#[allow(dead_code)]
pub struct AdminUser(pub Claims);

fn extract_claims(req: &HttpRequest) -> Result<Claims, AppError> {
    let config = req
        .app_data::<web::Data<Config>>()
        .ok_or(AppError::Unauthorized)?;

    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)?;

    verify_access_token(token, &config.jwt_secret).map_err(|_| AppError::Unauthorized)
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
        let claims = extract_claims(req);
        let pool = req.app_data::<web::Data<Pool>>().cloned();
        Box::pin(async move {
            let claims = claims?;
            let user_id = claims.user_id().map_err(|_| AppError::Unauthorized)?;
            let pool = pool.ok_or(AppError::Unauthorized)?;
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
