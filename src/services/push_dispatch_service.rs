use base64::Engine;
use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use std::net::IpAddr;
use uuid::Uuid;

use crate::config::{Config, PushDeliveryMode};
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{admin_service, push_token_service};

const APNS_PRODUCTION_URL: &str = "https://api.push.apple.com";
const APNS_SANDBOX_URL: &str = "https://api.sandbox.push.apple.com";
const RELAY_WEBHOOK_PATH: &str = "/v1/push/webhook";

#[derive(Debug, Clone)]
struct PushTargetData {
    platform: String,
    token: String,
}

#[derive(Debug, Serialize)]
struct PushTarget<'a> {
    platform: &'a str,
    token: &'a str,
}

#[derive(Debug, Serialize)]
struct NewMessagePushPayload<'a> {
    event: &'static str,
    recipient_id: Uuid,
    sender_id: Uuid,
    message_id: Uuid,
    message_preview: &'a str,
    targets: Vec<PushTarget<'a>>,
}

#[derive(Debug, Clone)]
struct ApnsConfig {
    team_id: String,
    key_id: String,
    bundle_id: String,
    private_key_pem: String,
    use_sandbox: bool,
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

pub async fn dispatch_new_message(
    pool: &Pool,
    cfg: &Config,
    recipient_id: Uuid,
    sender_id: Uuid,
    message_id: Uuid,
    message_content: &str,
) -> Result<(), AppError> {
    let configured_webhook_url =
        admin_service::get_setting(pool, admin_service::SETTING_WEBHOOK_URL)?
            .map(|item| item.value)
            .filter(|url| !url.trim().is_empty());
    let webhook_url =
        configured_webhook_url.or_else(|| derive_default_relay_webhook_url(&cfg.instance_domain));
    let tokens = push_token_service::list_tokens_for_user(pool, recipient_id)?;
    if tokens.is_empty() {
        return Ok(());
    }

    let preview = truncate_preview(message_content, 140);
    let all_targets = tokens
        .iter()
        .map(|item| PushTargetData {
            platform: item.platform.clone(),
            token: item.token.clone(),
        })
        .collect::<Vec<_>>();

    let mut ios_targets = Vec::new();
    let mut webhook_targets = Vec::new();
    for target in all_targets {
        if target.platform.eq_ignore_ascii_case("ios") {
            ios_targets.push(target);
        } else {
            webhook_targets.push(target);
        }
    }

    let mut dispatch_errors = Vec::new();

    match cfg.push_delivery_mode {
        PushDeliveryMode::Relay => {
            webhook_targets.extend(ios_targets);
        }
        PushDeliveryMode::Direct => {
            if !ios_targets.is_empty() {
                if let Some(apns_cfg) = parse_apns_config(cfg) {
                    if let Err(error) = send_apns_pushes(
                        &apns_cfg,
                        &ios_targets,
                        recipient_id,
                        sender_id,
                        message_id,
                        &preview,
                    )
                    .await
                    {
                        dispatch_errors.push(format!("APNs dispatch failed: {error}"));
                    }
                } else {
                    tracing::warn!(
                        recipient_id = %recipient_id,
                        ios_target_count = ios_targets.len(),
                        "APNs config missing in direct mode; skipping iOS push delivery"
                    );
                }
            }
        }
        PushDeliveryMode::Hybrid => {
            if !ios_targets.is_empty() {
                if let Some(apns_cfg) = parse_apns_config(cfg) {
                    if let Err(error) = send_apns_pushes(
                        &apns_cfg,
                        &ios_targets,
                        recipient_id,
                        sender_id,
                        message_id,
                        &preview,
                    )
                    .await
                    {
                        tracing::warn!(
                            recipient_id = %recipient_id,
                            ios_target_count = ios_targets.len(),
                            error = %error,
                            "APNs dispatch failed in hybrid mode; falling back to webhook for iOS targets"
                        );
                        webhook_targets.extend(ios_targets);
                    }
                } else {
                    tracing::warn!(
                        recipient_id = %recipient_id,
                        ios_target_count = ios_targets.len(),
                        "APNs config missing in hybrid mode; falling back to webhook targets for iOS"
                    );
                    webhook_targets.extend(ios_targets);
                }
            }
        }
    };

    if !webhook_targets.is_empty() {
        if let Some(url) = webhook_url {
            if let Err(error) = send_webhook_push(
                &url,
                &webhook_targets,
                recipient_id,
                sender_id,
                message_id,
                &preview,
            )
            .await
            {
                dispatch_errors.push(format!("Webhook dispatch failed: {error}"));
            }
        } else {
            tracing::warn!(
                recipient_id = %recipient_id,
                target_count = webhook_targets.len(),
                mode = ?cfg.push_delivery_mode,
                "notification_webhook_url not configured; skipping webhook push delivery"
            );
        }
    }

    if dispatch_errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Internal(anyhow::anyhow!(
            dispatch_errors.join("; ")
        )))
    }
}

fn derive_default_relay_webhook_url(instance_domain: &str) -> Option<String> {
    let trimmed = instance_domain.trim();
    if trimmed.is_empty() {
        return None;
    }

    let host = if let Ok(parsed) = reqwest::Url::parse(trimmed) {
        parsed.host_str().unwrap_or_default().to_string()
    } else {
        trimmed
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or_default()
            .split(':')
            .next()
            .unwrap_or_default()
            .to_string()
    };

    let host = host.trim().to_lowercase();
    if host.is_empty() || host == "localhost" || host.parse::<IpAddr>().is_ok() {
        return None;
    }

    Some(format!("https://push.{host}{RELAY_WEBHOOK_PATH}"))
}

fn truncate_preview(content: &str, max_chars: usize) -> String {
    if is_encrypted_payload(content) {
        return "Sent you an encrypted message".to_string();
    }
    let trimmed = content.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let mut out = String::new();
    for ch in trimmed.chars().take(max_chars) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn is_encrypted_payload(content: &str) -> bool {
    let trimmed = content.trim();
    if !trimmed.starts_with('{') {
        return false;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return false;
    };
    let Some(map) = value.as_object() else {
        return false;
    };
    map.get("v").and_then(|v| v.as_i64()) == Some(1)
        && map.get("recipient").and_then(|v| v.as_object()).is_some()
        && map.get("sender").and_then(|v| v.as_object()).is_some()
}

fn parse_apns_config(cfg: &Config) -> Option<ApnsConfig> {
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

async fn send_apns_pushes(
    cfg: &ApnsConfig,
    targets: &[PushTargetData],
    recipient_id: Uuid,
    sender_id: Uuid,
    message_id: Uuid,
    preview: &str,
) -> Result<(), AppError> {
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
    for target in targets {
        let url = format!("{base_url}/3/device/{}", target.token);
        let response: reqwest::Response = client
            .post(&url)
            .header("authorization", format!("bearer {auth_token}"))
            .header("apns-topic", cfg.bundle_id.as_str())
            .header("apns-push-type", "alert")
            .header("apns-priority", "10")
            .json(&payload)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("APNs send request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            failures.push(format!(
                "token={} status={} body={}",
                target.token, status, body
            ));
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

async fn send_webhook_push(
    webhook_url: &str,
    targets: &[PushTargetData],
    recipient_id: Uuid,
    sender_id: Uuid,
    message_id: Uuid,
    preview: &str,
) -> Result<(), AppError> {
    let payload = NewMessagePushPayload {
        event: "new_message",
        recipient_id,
        sender_id,
        message_id,
        message_preview: preview,
        targets: targets
            .iter()
            .map(|item| PushTarget {
                platform: item.platform.as_str(),
                token: item.token.as_str(),
            })
            .collect(),
    };

    let client = reqwest::Client::new();
    let response = client
        .post(webhook_url)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Push webhook send failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!(
            "Push webhook returned {}: {}",
            status,
            body
        )));
    }

    Ok(())
}
