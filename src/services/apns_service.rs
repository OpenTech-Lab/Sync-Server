use base64::Engine;
use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use uuid::Uuid;

use crate::config::Config;
use crate::errors::AppError;

const APNS_PRODUCTION_URL: &str = "https://api.push.apple.com";
const APNS_SANDBOX_URL: &str = "https://api.sandbox.push.apple.com";

#[derive(Debug, Clone)]
pub struct ApnsConfig {
    pub team_id: String,
    pub key_id: String,
    pub bundle_id: String,
    pub private_key_pem: String,
    pub use_sandbox: bool,
}

#[derive(Debug, Serialize)]
struct ApnsClaims<'a> {
    iss: &'a str,
    iat: i64,
}

#[derive(Debug, Serialize)]
struct ApnsPayload<'a> {
    aps: ApnsAps<'a>,
    message_id: String,
    sender_id: String,
    recipient_id: String,
}

#[derive(Debug, Serialize)]
struct ApnsAps<'a> {
    alert: ApnsAlert<'a>,
    sound: &'static str,
}

#[derive(Debug, Serialize)]
struct ApnsAlert<'a> {
    title: &'a str,
    body: &'a str,
}

pub fn parse_apns_config(cfg: &Config) -> Option<ApnsConfig> {
    let team_id = cfg.apns_team_id.as_ref()?.trim().to_string();
    let key_id = cfg.apns_key_id.as_ref()?.trim().to_string();
    let bundle_id = cfg.apns_bundle_id.as_ref()?.trim().to_string();
    let private_key_raw = cfg.apns_private_key_p8.as_ref()?.trim().to_string();
    if team_id.is_empty() || key_id.is_empty() || bundle_id.is_empty() || private_key_raw.is_empty()
    {
        return None;
    }

    let private_key_pem = normalize_apns_private_key(&private_key_raw).ok()?;
    Some(ApnsConfig {
        team_id,
        key_id,
        bundle_id,
        private_key_pem,
        use_sandbox: cfg.apns_use_sandbox,
    })
}

pub async fn send_alert_to_tokens(
    cfg: &ApnsConfig,
    tokens: &[String],
    recipient_id: Uuid,
    sender_id: Uuid,
    message_id: Uuid,
    preview: &str,
) -> Result<(), AppError> {
    if tokens.is_empty() {
        return Ok(());
    }

    let base_url = if cfg.use_sandbox {
        APNS_SANDBOX_URL
    } else {
        APNS_PRODUCTION_URL
    };
    let auth_token = make_apns_jwt(cfg)?;
    let payload = ApnsPayload {
        aps: ApnsAps {
            alert: ApnsAlert {
                title: "Sync",
                body: preview,
            },
            sound: "default",
        },
        message_id: message_id.to_string(),
        sender_id: sender_id.to_string(),
        recipient_id: recipient_id.to_string(),
    };

    let client = reqwest::Client::new();
    let mut failures = Vec::new();
    for token in tokens {
        let url = format!("{base_url}/3/device/{token}");
        // apns-expiration: Unix timestamp after which APNs discards the
        // notification if it has not yet been delivered.  Default (0) means
        // "deliver once immediately or discard", which silently drops the
        // notification when the device is temporarily unreachable.  Using a
        // 24-hour window ensures delivery after brief offline periods.
        let expiration = (Utc::now().timestamp() + 86_400).to_string();
        let response: reqwest::Response = client
            .post(&url)
            .version(reqwest::Version::HTTP_2)
            .header("authorization", format!("bearer {auth_token}"))
            .header("apns-topic", cfg.bundle_id.as_str())
            .header("apns-push-type", "alert")
            .header("apns-priority", "10")
            .header("apns-expiration", expiration.as_str())
            .json(&payload)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| {
                AppError::Internal(anyhow::anyhow!(
                    "APNs send request failed: {}; debug={:?}; url={}",
                    e,
                    e,
                    url
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            failures.push(format!("token={token} status={status} body={body}"));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppError::Internal(anyhow::anyhow!(
            "APNs returned errors for {} target(s): {}",
            failures.len(),
            failures.join(" | ")
        )))
    }
}

fn normalize_apns_private_key(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.contains("-----BEGIN") {
        return Ok(trimmed.replace("\\n", "\n"));
    }

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(trimmed.as_bytes())
        .map_err(|e| AppError::BadRequest(format!("Invalid APNS_PRIVATE_KEY_P8 base64: {e}")))?;
    let decoded_str = String::from_utf8(decoded)
        .map_err(|e| AppError::BadRequest(format!("Invalid APNS_PRIVATE_KEY_P8 UTF-8: {e}")))?;
    Ok(decoded_str.replace("\\n", "\n"))
}

fn make_apns_jwt(cfg: &ApnsConfig) -> Result<String, AppError> {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(cfg.key_id.clone());
    let claims = ApnsClaims {
        iss: cfg.team_id.as_str(),
        iat: Utc::now().timestamp(),
    };
    let key = EncodingKey::from_ec_pem(cfg.private_key_pem.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Invalid APNs private key: {e}")))?;
    encode(&header, &claims, &key)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to encode APNs JWT: {e}")))
}
