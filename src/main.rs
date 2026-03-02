mod config;
mod db;
mod errors;
mod routes;

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
    let pool = db::create_pool(&config.database_url)
        .expect("Failed to create database connection pool");

    // Run pending migrations on startup
    db::run_migrations(&pool).expect("Failed to run database migrations");

    tracing::info!(
        host = %host,
        port,
        instance = %config.instance_name,
        "sync-server starting"
    );

    let pool_data = web::Data::new(pool);

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .app_data(pool_data.clone())
            .configure(routes::configure)
    })
    .bind((host.as_str(), port))?
    .run()
    .await
}
