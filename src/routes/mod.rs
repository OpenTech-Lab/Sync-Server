pub mod health;

use actix_web::web;

/// Register all application routes.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(health::liveness))
        .route("/ready", web::get().to(health::readiness));
}
