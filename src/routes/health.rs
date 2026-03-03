use actix_web::{web, HttpResponse};
use diesel::prelude::*;
use serde::Serialize;

use crate::config::Config;
use crate::db::Pool;
use crate::services::admin_service;
use crate::services::geoip_service::PlanetGeoInfo;

#[derive(Serialize)]
struct LivenessResponse {
    status: &'static str,
    version: &'static str,
    instance_name: String,
    instance_domain: String,
    instance_description: Option<String>,
    country_code: Option<String>,
    country_name: Option<String>,
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
pub async fn liveness(
    cfg: web::Data<Config>,
    geo: web::Data<PlanetGeoInfo>,
    pool: web::Data<Pool>,
) -> HttpResponse {
    let planet_name = admin_service::get_setting(&pool, admin_service::SETTING_PLANET_NAME)
        .ok()
        .flatten()
        .map(|s| s.value)
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| cfg.instance_name.clone());
    let planet_description =
        admin_service::get_setting(&pool, admin_service::SETTING_PLANET_DESCRIPTION)
            .ok()
            .flatten()
            .map(|s| s.value)
            .filter(|v| !v.trim().is_empty());

    HttpResponse::Ok().json(LivenessResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        instance_name: planet_name,
        instance_domain: cfg.instance_domain.clone(),
        instance_description: planet_description,
        country_code: geo.country_code.clone(),
        country_name: geo.country_name.clone(),
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
