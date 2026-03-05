use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::config::Config;
use crate::errors::AppError;
use crate::services::apns_service;

#[derive(Debug, Deserialize)]
struct RelayTarget {
    platform: String,
    token: String,
}

#[derive(Debug, Deserialize)]
struct RelayPayload {
    event: String,
    recipient_id: Option<Uuid>,
    sender_id: Option<Uuid>,
    message_id: Option<Uuid>,
    message_preview: Option<String>,
    targets: Vec<RelayTarget>,
}

async fn webhook(
    req: HttpRequest,
    cfg: web::Data<Config>,
    body: web::Json<RelayPayload>,
) -> Result<HttpResponse, AppError> {
    if let Some(expected_secret) = cfg.push_relay_shared_secret.as_ref() {
        let provided = req
            .headers()
            .get("x-sync-push-secret")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if provided != expected_secret {
            return Err(AppError::Unauthorized);
        }
    }

    if body.event.trim() != "new_message" {
        return Ok(HttpResponse::NoContent().finish());
    }

    let ios_tokens: Vec<String> = body
        .targets
        .iter()
        .filter(|item| item.platform.eq_ignore_ascii_case("ios") && !item.token.trim().is_empty())
        .map(|item| item.token.trim().to_string())
        .collect();
    if ios_tokens.is_empty() {
        return Ok(HttpResponse::NoContent().finish());
    }

    let apns_cfg = apns_service::parse_apns_config(&cfg)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Relay APNs config is missing")))?;
    apns_service::send_alert_to_tokens(
        &apns_cfg,
        &ios_tokens,
        body.recipient_id.unwrap_or_else(Uuid::nil),
        body.sender_id.unwrap_or_else(Uuid::nil),
        body.message_id.unwrap_or_else(Uuid::nil),
        body.message_preview
            .as_deref()
            .unwrap_or("You have a new message"),
    )
    .await?;

    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/webhook", web::post().to(webhook));
}
