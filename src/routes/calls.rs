use actix_web::{web, HttpResponse};

use crate::auth::middleware::AuthUser;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::call_service;

async fn call_history(
    pool: web::Data<Pool>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    let user_id = auth.0.user_id()?;
    let records = call_service::history_for_user(&pool, user_id, 50)?;
    Ok(HttpResponse::Ok().json(records))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::get().to(call_history));
}
