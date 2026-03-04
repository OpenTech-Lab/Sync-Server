use actix_web::{web, HttpResponse};
use base64::Engine;
use diesel::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use crate::config::Config;
use crate::db::Pool;
use crate::schema::users::dsl as user_dsl;
use crate::services::admin_service;
use crate::services::geoip_service::PlanetGeoInfo;

#[derive(Serialize)]
struct LivenessResponse {
    status: &'static str,
    version: &'static str,
    instance_name: String,
    instance_domain: String,
    instance_description: Option<String>,
    instance_image_url: Option<String>,
    member_count: i64,
    linked_planets: Vec<String>,
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
    let mut conn = match pool.get() {
        Ok(conn) => conn,
        Err(_) => {
            return HttpResponse::Ok().json(LivenessResponse {
                status: "ok",
                version: env!("CARGO_PKG_VERSION"),
                instance_name: cfg.instance_name.clone(),
                instance_domain: cfg.instance_domain.clone(),
                instance_description: None,
                instance_image_url: None,
                member_count: 0,
                linked_planets: vec![],
                country_code: geo.country_code.clone(),
                country_name: geo.country_name.clone(),
            })
        }
    };
    let member_count = user_dsl::users.count().get_result(&mut conn).unwrap_or(0);

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
    let has_planet_image =
        admin_service::get_setting(&pool, admin_service::SETTING_PLANET_IMAGE_BASE64)
            .ok()
            .flatten()
            .map(|s| !s.value.trim().is_empty())
            .unwrap_or(false);
    let linked_planets = admin_service::read_linked_planets(&pool).unwrap_or_default();

    HttpResponse::Ok().json(LivenessResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        instance_name: planet_name,
        instance_domain: cfg.instance_domain.clone(),
        instance_description: planet_description,
        instance_image_url: has_planet_image.then_some("/planet-image".to_string()),
        member_count,
        linked_planets,
        country_code: geo.country_code.clone(),
        country_name: geo.country_name.clone(),
    })
}

#[derive(Deserialize)]
struct ParsedDataUrl {
    content_type: String,
    payload_base64: String,
}

fn parse_image_data_url(raw: &str) -> Option<ParsedDataUrl> {
    let trimmed = raw.trim();
    let payload = if let Some(value) = trimmed.strip_prefix("data:image/jpeg;base64,") {
        ParsedDataUrl {
            content_type: "image/jpeg".to_string(),
            payload_base64: value.to_string(),
        }
    } else if let Some(value) = trimmed.strip_prefix("data:image/png;base64,") {
        ParsedDataUrl {
            content_type: "image/png".to_string(),
            payload_base64: value.to_string(),
        }
    } else if let Some(value) = trimmed.strip_prefix("data:image/webp;base64,") {
        ParsedDataUrl {
            content_type: "image/webp".to_string(),
            payload_base64: value.to_string(),
        }
    } else {
        return None;
    };
    Some(payload)
}

/// GET /planet-image — public planet image for onboarding cards
pub async fn planet_image(pool: web::Data<Pool>) -> HttpResponse {
    let Some(image_setting) =
        admin_service::get_setting(&pool, admin_service::SETTING_PLANET_IMAGE_BASE64)
            .ok()
            .flatten()
            .map(|s| s.value)
            .filter(|v| !v.trim().is_empty())
    else {
        return HttpResponse::NotFound().finish();
    };

    let Some(parsed) = parse_image_data_url(&image_setting) else {
        return HttpResponse::NotFound().finish();
    };

    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(parsed.payload_base64) else {
        return HttpResponse::NotFound().finish();
    };

    HttpResponse::Ok()
        .insert_header(("Cache-Control", "public, max-age=300"))
        .content_type(parsed.content_type)
        .body(bytes)
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
