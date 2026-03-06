use actix_web::{web, HttpResponse};
use altcha_lib_rs::{create_challenge, ChallengeOptions};
use serde::Serialize;

use crate::config::Config;
use crate::errors::AppError;

#[derive(Debug, Serialize)]
pub struct AltchaChallengeResponse {
    pub algorithm: String,
    pub challenge: String,
    pub salt: String,
    pub signature: String,
}

/// GET /auth/altcha
/// Generates a new ALTCHA challenge. Returns 404 if ALTCHA is not configured.
pub async fn get_altcha_challenge(config: web::Data<Config>) -> Result<HttpResponse, AppError> {
    let hmac_key = match &config.altcha_hmac_key {
        Some(key) => key,
        None => return Err(AppError::NotFound),
    };

    let options = ChallengeOptions {
        hmac_key: hmac_key,
        max_number: None, // use default
        expires: Some(chrono::Utc::now() + chrono::Duration::minutes(5)), // 5 mins expiration
        salt_length: None, // use default
        ..Default::default()
    };

    let challenge = create_challenge(options)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to generate altcha challenge: {}", e)))?;

    Ok(HttpResponse::Ok().json(AltchaChallengeResponse {
        algorithm: challenge.algorithm.to_string(),
        challenge: challenge.challenge,
        salt: challenge.salt,
        signature: challenge.signature,
    }))
}
