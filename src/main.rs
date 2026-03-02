mod auth;
mod config;
mod db;
mod errors;
mod models;
mod routes;
mod schema;
mod services;
mod ws;

use actix_governor::{Governor, GovernorConfigBuilder};
use actix_web::{web, App, HttpServer};
use tracing_actix_web::TracingLogger;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;

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
    let host = config.server_host.clone();
    let port = config.server_port;

    // Log host hardware specs and suggested user cap
    config.log_host_spec();

    // Create DB connection pool
    let pool =
        db::create_pool(&config.database_url).expect("Failed to create database connection pool");

    // Run pending migrations on startup
    db::run_migrations(&pool).expect("Failed to run database migrations");

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

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .app_data(pool_data.clone())
            .app_data(config_data.clone())
            .app_data(redis_data.clone())
            // ── Health checks (unauthenticated, no rate-limiting) ──────────
            .route("/health", web::get().to(routes::health::liveness))
            .route("/ready", web::get().to(routes::health::readiness))
            // ── Federation routes (ActivityPub / WebFinger) ────────────────
            .configure(routes::federation::configure_public)
            .service(
                web::scope("")
                    .wrap(Governor::new(&federation_inbox_governor))
                    .configure(routes::federation::configure_inbox),
            )
            // ── Auth routes: 5 req/s, burst 10 ────────────────────────────
            .service(
                web::scope("/auth")
                    .wrap(Governor::new(&auth_governor))
                    .configure(routes::auth::configure),
            )
            // ── Messaging REST API: 60 req/s, burst 100 ───────────────────
            .service(
                web::scope("/api/messages")
                    .wrap(Governor::new(&api_governor))
                    .configure(routes::messages::configure),
            )
            // ── WebSocket upgrade (auth inside handler, no HTTP rate-limit) ─
            .service(web::scope("/ws").configure(routes::ws::configure))
    })
    .bind((host.as_str(), port))?
    .run()
    .await
}
