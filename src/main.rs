mod auth;
mod config;
mod db;
mod errors;
mod models;
mod routes;
mod schema;
mod services;
mod ws;

use actix_cors::Cors;
use actix_governor::{Governor, GovernorConfigBuilder};
use actix_web::{http::header, web, App, HttpServer};
use tracing_actix_web::TracingLogger;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use services::geoip_service::PlanetGeoInfo;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load .env file in development (ignore missing file in production)
    let _ = dotenvy::dotenv();

    // Structured JSON logging via tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sync_server=info,actix_web=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    // Load configuration from environment
    let config = Config::from_env().expect("Failed to load server configuration");
    config
        .validate_security_defaults()
        .expect("Security baseline validation failed");
    let host = config.server_host.clone();
    let port = config.server_port;

    // Log host hardware specs and suggested user cap
    config.log_host_spec();

    // Create DB connection pool
    let pool =
        db::create_pool(&config.database_url).expect("Failed to create database connection pool");

    // Run pending migrations on startup
    db::run_migrations(&pool).expect("Failed to run database migrations");
    routes::auth::initialize_first_admin_setup_link(&pool, &config)
        .expect("Failed to initialize one-time admin setup URL");

    // Create Redis client (connection is lazy; verified on first use)
    let redis = redis::Client::open(config.redis_url.as_str()).expect("Invalid Redis URL");

    tracing::info!(
        host = %host,
        port,
        instance = %config.instance_name,
        "sync-server starting"
    );

    // ── Rate-limit configurations ──────────────────────────────────────────────
    // /auth/*: 5 req/s per IP, burst 10  (brute-force protection)
    let auth_governor = GovernorConfigBuilder::default()
        .requests_per_second(5)
        .burst_size(10)
        .finish()
        .expect("Invalid auth governor config");

    // /api/*: 60 req/s per IP, burst 100
    // POST /api/messages is covered here; stricter per-route limiting can be
    // layered on top via custom middleware in a future phase if needed.
    let api_governor = GovernorConfigBuilder::default()
        .requests_per_second(60)
        .burst_size(100)
        .finish()
        .expect("Invalid API governor config");

    // /users/*/inbox: federated inbound traffic, limited per source IP
    let federation_inbox_governor = GovernorConfigBuilder::default()
        .requests_per_second(config.federation_inbox_rps)
        .burst_size(config.federation_inbox_burst)
        .finish()
        .expect("Invalid federation inbox governor config");

    let pool_data = web::Data::new(pool);
    let config_data = web::Data::new(config);
    let redis_data = web::Data::new(redis);
    let geo_info_data = web::Data::new(PlanetGeoInfo::detect(
        &config_data.instance_domain,
        std::path::Path::new("data/ip-to-country.mmdb"),
    ));

    HttpServer::new(move || {
        // Allow any localhost origin (Flutter web dev) plus configured origins.
        // In production, restrict allowed_origin to your actual domain.
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"])
            .allowed_headers(vec![
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::ACCEPT,
            ])
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(TracingLogger::default())
            // Allow larger JSON payloads for admin image upload data URLs.
            .app_data(web::JsonConfig::default().limit(35 * 1024 * 1024))
            .app_data(pool_data.clone())
            .app_data(config_data.clone())
            .app_data(redis_data.clone())
            .app_data(geo_info_data.clone())
            // ── Health checks (unauthenticated, no rate-limiting) ──────────
            .route("/health", web::get().to(routes::health::liveness))
            .route("/planet-image", web::get().to(routes::health::planet_image))
            .route("/ready", web::get().to(routes::health::readiness))
            // ── Federation routes (ActivityPub / WebFinger) ────────────────
            .configure(routes::federation::configure_public)
            // ── Auth routes: 5 req/s, burst 10 ────────────────────────────
            .service(
                web::scope("/auth")
                    .wrap(Governor::new(&auth_governor))
                    .route(
                        "/altcha",
                        web::get().to(routes::altcha::get_altcha_challenge),
                    )
                    .configure(routes::auth::configure),
            )
            // ── Messaging REST API: 60 req/s, burst 100 ───────────────────
            .service(
                web::scope("/api/messages")
                    .wrap(Governor::new(&api_governor))
                    .configure(routes::messages::configure),
            )
            .service(
                web::scope("/api/backup")
                    .wrap(Governor::new(&api_governor))
                    .configure(routes::backup::configure),
            )
            .service(
                web::scope("/api/admin")
                    .wrap(Governor::new(&api_governor))
                    .configure(routes::admin::configure),
            )
            .service(
                web::scope("/api/stickers")
                    .wrap(Governor::new(&api_governor))
                    .configure(routes::stickers::configure),
            )
            .service(
                web::scope("/api/planet-news")
                    .wrap(Governor::new(&api_governor))
                    .configure(routes::planet_news::configure),
            )
            .service(
                web::scope("/api/profile")
                    .wrap(Governor::new(&api_governor))
                    .configure(routes::profile::configure),
            )
            .service(
                web::scope("/api/push")
                    .wrap(Governor::new(&api_governor))
                    .configure(routes::push::configure),
            )
            .service(
                web::scope("/v1/push")
                    .wrap(Governor::new(&api_governor))
                    .configure(routes::push_relay::configure),
            )
            // ── WebSocket upgrade (auth inside handler, no HTTP rate-limit) ─
            .service(web::scope("/ws").configure(routes::ws::configure))
            // ── Federation inbox: scope("") must be LAST — empty prefix     ──
            // ── matches any URL in Actix-web, so all named scopes above     ──
            // ── must be registered first so they win on their own paths.    ──
            .service(
                web::scope("")
                    .wrap(Governor::new(&federation_inbox_governor))
                    .configure(routes::federation::configure_inbox),
            )
    })
    .bind((host.as_str(), port))?
    .run()
    .await
}
