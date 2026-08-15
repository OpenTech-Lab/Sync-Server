use serde::Serialize;
use uuid::Uuid;

use crate::config::{Config, PushDeliveryMode};
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{admin_service, apns_service, push_token_service};

/// Default push relay hosted by the app publisher. Servers that have not
/// configured a custom `notification_webhook_url` will forward iOS push
/// payloads here so that APNs delivery works for the official mobile app
/// without each operator having to supply their own APNs credentials.
const DEFAULT_RELAY_WEBHOOK_URL: &str = "https://push.sync.icyanstudio.net/v1/push/webhook";

#[derive(Debug, Clone)]
struct PushTargetData {
    platform: String,
    token: String,
    token_kind: String,
}

#[derive(Debug, Serialize)]
struct PushTarget<'a> {
    platform: &'a str,
    token: &'a str,
    token_kind: &'a str,
}

#[derive(Debug, Serialize)]
struct IncomingCallPushPayload<'a> {
    event: &'static str,
    recipient_id: Uuid,
    caller_id: Uuid,
    caller_display_name: &'a str,
    call_id: Uuid,
    call_type: &'a str,
    targets: Vec<PushTarget<'a>>,
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
    // Fall back to the publisher-hosted relay when no custom URL is configured.
    let webhook_url =
        configured_webhook_url.or_else(|| Some(DEFAULT_RELAY_WEBHOOK_URL.to_string()));
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
            token_kind: item.token_kind.clone(),
        })
        .collect::<Vec<_>>();

    let mut ios_targets = Vec::new();
    let mut webhook_targets = Vec::new();
    for target in all_targets {
        if target.platform.eq_ignore_ascii_case("ios") {
            ios_targets.push(target);
        } else if is_relay_deliverable_platform(&target.platform) {
            // Only include platforms the push relay can actually deliver to
            // (e.g. android). Desktop/web clients (linux, windows, macos, web)
            // receive messages via their live SSE/WebSocket connection and do
            // not have a relay-reachable push endpoint.
            webhook_targets.push(target);
        } else {
            tracing::debug!(
                platform = %target.platform,
                "Skipping push dispatch for non-relay platform (real-time delivery only)"
            );
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
                    tracing::info!(
                        recipient_id = %recipient_id,
                        ios_target_count = ios_targets.len(),
                        use_sandbox = apns_cfg.use_sandbox,
                        bundle_id = %apns_cfg.bundle_id,
                        "Attempting APNs direct push"
                    );
                    match apns_service::send_alert_to_tokens(
                        &apns_cfg,
                        &tokens,
                        recipient_id,
                        sender_id,
                        message_id,
                        &preview,
                    )
                    .await
                    {
                        Ok(()) => {
                            tracing::info!(
                                recipient_id = %recipient_id,
                                ios_target_count = ios_targets.len(),
                                "APNs direct push delivered successfully"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(
                                recipient_id = %recipient_id,
                                ios_target_count = ios_targets.len(),
                                error = %error,
                                "APNs direct push failed"
                            );
                            dispatch_errors.push(format!("APNs dispatch failed: {error}"));
                        }
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

pub async fn dispatch_incoming_call(
    pool: &Pool,
    cfg: &Config,
    callee_id: Uuid,
    caller_id: Uuid,
    caller_display_name: &str,
    call_id: Uuid,
    call_type: &str,
) -> Result<(), AppError> {
    let configured_webhook_url =
        admin_service::get_setting(pool, admin_service::SETTING_WEBHOOK_URL)?
            .map(|item| item.value)
            .filter(|url| !url.trim().is_empty());
    let webhook_url =
        configured_webhook_url.or_else(|| Some(DEFAULT_RELAY_WEBHOOK_URL.to_string()));
    let tokens = push_token_service::list_tokens_for_user(pool, callee_id)?;
    if tokens.is_empty() {
        return Ok(());
    }

    let all_targets = tokens
        .iter()
        .map(|item| PushTargetData {
            platform: item.platform.clone(),
            token: item.token.clone(),
            token_kind: item.token_kind.clone(),
        })
        .collect::<Vec<_>>();

    let mut ios_targets = Vec::new();
    let mut webhook_targets = Vec::new();
    for target in all_targets {
        if target.platform.eq_ignore_ascii_case("ios") {
            ios_targets.push(target);
        } else if is_relay_deliverable_platform(&target.platform) {
            webhook_targets.push(target);
        }
    }

    // Within the iOS targets, VoIP-kind tokens get the PushKit VoIP push
    // (wakes the app for CallKit); default-kind tokens only get the
    // alert-style call push as a fallback when no VoIP token exists for this
    // user. A device registers both kinds, so without this the same device
    // would get the alert push *and* the VoIP push for the same call — the
    // stray alert notification then gets recorded as a "missed call" on the
    // client even when the user answered via CallKit.
    let (voip_ios_targets, mut default_ios_targets): (Vec<_>, Vec<_>) = ios_targets
        .into_iter()
        .partition(|t| t.token_kind == "voip");
    if !voip_ios_targets.is_empty() {
        default_ios_targets.clear();
    }

    let mut dispatch_errors = Vec::new();

    match cfg.push_delivery_mode {
        PushDeliveryMode::Relay => {
            webhook_targets.extend(voip_ios_targets);
            webhook_targets.extend(default_ios_targets);
        }
        PushDeliveryMode::Direct => {
            if let Some(apns_cfg) = apns_service::parse_apns_config(cfg) {
                if !voip_ios_targets.is_empty() {
                    let tokens: Vec<String> =
                        voip_ios_targets.iter().map(|t| t.token.clone()).collect();
                    if let Err(error) = apns_service::send_voip_call_push_to_tokens(
                        &apns_cfg,
                        &tokens,
                        callee_id,
                        caller_id,
                        caller_display_name,
                        call_id,
                        call_type,
                    )
                    .await
                    {
                        dispatch_errors.push(format!("APNs VoIP call dispatch failed: {error}"));
                    }
                }
                if !default_ios_targets.is_empty() {
                    let tokens: Vec<String> = default_ios_targets
                        .iter()
                        .map(|t| t.token.clone())
                        .collect();
                    if let Err(error) = apns_service::send_call_alert_to_tokens(
                        &apns_cfg, &tokens, callee_id, caller_id, call_id, call_type,
                    )
                    .await
                    {
                        dispatch_errors.push(format!("APNs call dispatch failed: {error}"));
                    }
                }
            }
        }
        PushDeliveryMode::Hybrid => {
            if let Some(apns_cfg) = apns_service::parse_apns_config(cfg) {
                if !voip_ios_targets.is_empty() {
                    let tokens: Vec<String> =
                        voip_ios_targets.iter().map(|t| t.token.clone()).collect();
                    if apns_service::send_voip_call_push_to_tokens(
                        &apns_cfg,
                        &tokens,
                        callee_id,
                        caller_id,
                        caller_display_name,
                        call_id,
                        call_type,
                    )
                    .await
                    .is_err()
                    {
                        webhook_targets.extend(voip_ios_targets);
                    }
                }
                if !default_ios_targets.is_empty() {
                    let tokens: Vec<String> = default_ios_targets
                        .iter()
                        .map(|t| t.token.clone())
                        .collect();
                    if apns_service::send_call_alert_to_tokens(
                        &apns_cfg, &tokens, callee_id, caller_id, call_id, call_type,
                    )
                    .await
                    .is_err()
                    {
                        webhook_targets.extend(default_ios_targets);
                    }
                }
            } else {
                webhook_targets.extend(voip_ios_targets);
                webhook_targets.extend(default_ios_targets);
            }
        }
    };

    if !webhook_targets.is_empty() {
        if let Some(url) = webhook_url {
            let call = CallPushContext {
                callee_id,
                caller_id,
                caller_display_name,
                call_id,
                call_type,
            };
            if let Err(error) =
                send_call_webhook_push(cfg, &url, &webhook_targets, &call).await
            {
                dispatch_errors.push(format!("Webhook call dispatch failed: {error}"));
            }
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

/// Returns true for mobile platforms that a webhook push relay can reach via
/// APNs or FCM-style delivery.  Desktop and web platforms (linux, windows,
/// macos, web) rely on a persistent SSE/WebSocket connection for real-time
/// delivery and have no relay-reachable push endpoint.
fn is_relay_deliverable_platform(platform: &str) -> bool {
    let p = platform.trim().to_lowercase();
    matches!(p.as_str(), "android" | "fcm")
}

fn truncate_preview(content: &str, max_chars: usize) -> String {
    if is_encrypted_payload(content) {
        return "Sent you a new message".to_string();
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
                token_kind: item.token_kind.as_str(),
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

/// Bundles the identifying fields of an in-flight call so functions that
/// dispatch a call push don't need a long, error-prone positional argument
/// list (clippy flags anything past 7 plain arguments).
struct CallPushContext<'a> {
    callee_id: Uuid,
    caller_id: Uuid,
    caller_display_name: &'a str,
    call_id: Uuid,
    call_type: &'a str,
}

async fn send_call_webhook_push(
    cfg: &Config,
    webhook_url: &str,
    targets: &[PushTargetData],
    call: &CallPushContext<'_>,
) -> Result<(), AppError> {
    let payload = IncomingCallPushPayload {
        event: "incoming_call",
        recipient_id: call.callee_id,
        caller_id: call.caller_id,
        caller_display_name: call.caller_display_name,
        call_id: call.call_id,
        call_type: call.call_type,
        targets: targets
            .iter()
            .map(|item| PushTarget {
                platform: item.platform.as_str(),
                token: item.token.as_str(),
                token_kind: item.token_kind.as_str(),
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
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Call push webhook send failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!(
            "Call push webhook returned {}: {}",
            status,
            body
        )));
    }

    Ok(())
}
