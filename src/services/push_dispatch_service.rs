use serde::Serialize;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{admin_service, push_token_service};

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
    recipient_id: Uuid,
    sender_id: Uuid,
    message_id: Uuid,
    message_content: &str,
) -> Result<(), AppError> {
    let webhook_url = admin_service::get_setting(pool, admin_service::SETTING_WEBHOOK_URL)?
        .map(|item| item.value)
        .filter(|url| !url.trim().is_empty());
    let Some(webhook_url) = webhook_url else {
        return Ok(());
    };

    let tokens = push_token_service::list_tokens_for_user(pool, recipient_id)?;
    if tokens.is_empty() {
        return Ok(());
    }

    let preview = truncate_preview(message_content, 140);
    let targets = tokens
        .iter()
        .map(|item| PushTarget {
            platform: item.platform.as_str(),
            token: item.token.as_str(),
        })
        .collect::<Vec<_>>();

    let payload = NewMessagePushPayload {
        event: "new_message",
        recipient_id,
        sender_id,
        message_id,
        message_preview: preview.as_str(),
        targets,
    };

    let client = reqwest::Client::new();
    let response = client
        .post(webhook_url)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Push webhook send failed: {}", e)))?;

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
