use serde::Serialize;
use std::net::IpAddr;
use uuid::Uuid;

use crate::config::{Config, PushDeliveryMode};
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{admin_service, apns_service, push_token_service};

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
                if let Some(apns_cfg) = apns_service::parse_apns_config(cfg) {
                    let tokens: Vec<String> = ios_targets.iter().map(|t| t.token.clone()).collect();
                    if let Err(error) = apns_service::send_alert_to_tokens(
                        &apns_cfg,
                        &tokens,
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
                if let Some(apns_cfg) = apns_service::parse_apns_config(cfg) {
                    let tokens: Vec<String> = ios_targets.iter().map(|t| t.token.clone()).collect();
                    if let Err(error) = apns_service::send_alert_to_tokens(
                        &apns_cfg,
                        &tokens,
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
                cfg,
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

async fn send_webhook_push(
    cfg: &Config,
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
    let mut request = client
        .post(webhook_url)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(5));
    if let Some(secret) = cfg.push_relay_shared_secret.as_ref() {
        request = request.header("x-sync-push-secret", secret.as_str());
    }
    let response = request
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
