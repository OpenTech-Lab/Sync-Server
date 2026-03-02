use actix_web::{web, HttpResponse};
use diesel::prelude::*;
use serde::Serialize;

use crate::db::Pool;

#[derive(Serialize)]
struct LivenessResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct ReadinessOk {
    status: &'static str,
    database: &'static str,
}

#[derive(Serialize)]
struct ReadinessErr {
    status: &'static str,
    database: &'static str,
    error: String,
}

/// GET /health — liveness probe (always 200 if the process is up)
pub async fn liveness() -> HttpResponse {
    HttpResponse::Ok().json(LivenessResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// GET /ready — readiness probe (200 only if the DB is reachable)
pub async fn readiness(pool: web::Data<Pool>) -> HttpResponse {
    match pool.get() {
        Ok(mut conn) => match diesel::sql_query("SELECT 1").execute(&mut conn) {
            Ok(_) => HttpResponse::Ok().json(ReadinessOk {
                status: "ready",
                database: "ok",
            }),
            Err(e) => HttpResponse::ServiceUnavailable().json(ReadinessErr {
                status: "not_ready",
                database: "error",
                error: e.to_string(),
            }),
        },
        Err(e) => HttpResponse::ServiceUnavailable().json(ReadinessErr {
            status: "not_ready",
            database: "error",
            error: e.to_string(),
        }),
    }
}
