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
    #[serde(default = "default_token_kind")]
    token_kind: String,
}

fn default_token_kind() -> String {
    "default".to_string()
}

#[derive(Debug, Deserialize)]
struct RelayPayload {
    event: String,
    recipient_id: Option<Uuid>,
    sender_id: Option<Uuid>,
    message_id: Option<Uuid>,
    message_preview: Option<String>,
    caller_id: Option<Uuid>,
    caller_display_name: Option<String>,
    call_id: Option<Uuid>,
    call_type: Option<String>,
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

    match body.event.trim() {
        "new_message" => {}
        "incoming_call" => return incoming_call_webhook(&cfg, &body).await,
        _ => return Ok(HttpResponse::NoContent().finish()),
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

    let apns_cfg = apns_service::parse_apns_config(&cfg).ok_or_else(|| {
        tracing::error!(
            target_count = body.targets.len(),
            ios_target_count = ios_tokens.len(),
            "Relay APNs config is missing"
        );
        AppError::Internal(anyhow::anyhow!("Relay APNs config is missing"))
    })?;
    if let Err(error) = apns_service::send_alert_to_tokens(
        &apns_cfg,
        &ios_tokens,
        body.recipient_id.unwrap_or_else(Uuid::nil),
        body.sender_id.unwrap_or_else(Uuid::nil),
        body.message_id.unwrap_or_else(Uuid::nil),
        body.message_preview
            .as_deref()
            .unwrap_or("You have a new message"),
    )
    .await
    {
        tracing::error!(
            ios_target_count = ios_tokens.len(),
            error = %error,
            error_debug = ?error,
            "Relay APNs send failed"
        );
        return Err(error);
    }
    tracing::info!(
        ios_target_count = ios_tokens.len(),
        recipient_id = ?body.recipient_id,
        sender_id = ?body.sender_id,
        message_id = ?body.message_id,
        apns_use_sandbox = apns_cfg.use_sandbox,
        "Relay APNs send success"
    );

    Ok(HttpResponse::NoContent().finish())
}

async fn incoming_call_webhook(
    cfg: &Config,
    body: &RelayPayload,
) -> Result<HttpResponse, AppError> {
    let voip_tokens: Vec<String> = body
        .targets
        .iter()
        .filter(|item| {
            item.platform.eq_ignore_ascii_case("ios")
                && item.token_kind == "voip"
                && !item.token.trim().is_empty()
        })
        .map(|item| item.token.trim().to_string())
        .collect();
    let default_tokens: Vec<String> = body
        .targets
        .iter()
        .filter(|item| {
            item.platform.eq_ignore_ascii_case("ios")
                && item.token_kind != "voip"
                && !item.token.trim().is_empty()
        })
        .map(|item| item.token.trim().to_string())
        .collect();

    if voip_tokens.is_empty() && default_tokens.is_empty() {
        return Ok(HttpResponse::NoContent().finish());
    }

    let apns_cfg = apns_service::parse_apns_config(cfg).ok_or_else(|| {
        tracing::error!(
            target_count = body.targets.len(),
            "Relay APNs config is missing for incoming_call event"
        );
        AppError::Internal(anyhow::anyhow!("Relay APNs config is missing"))
    })?;

    let recipient_id = body.recipient_id.unwrap_or_else(Uuid::nil);
    let caller_id = body.caller_id.unwrap_or_else(Uuid::nil);
    let call_id = body.call_id.unwrap_or_else(Uuid::nil);
    let call_type = body.call_type.as_deref().unwrap_or("voice");
    let caller_display_name = body.caller_display_name.as_deref().unwrap_or("");

    let mut errors = Vec::new();

    if !voip_tokens.is_empty() {
        if let Err(error) = apns_service::send_voip_call_push_to_tokens(
            &apns_cfg,
            &voip_tokens,
            recipient_id,
            caller_id,
            caller_display_name,
            call_id,
            call_type,
        )
        .await
        {
            tracing::error!(
                voip_target_count = voip_tokens.len(),
                error = %error,
                "Relay APNs VoIP call send failed"
            );
            errors.push(error.to_string());
        }
    }

    if !default_tokens.is_empty() {
        if let Err(error) = apns_service::send_call_alert_to_tokens(
            &apns_cfg,
            &default_tokens,
            recipient_id,
            caller_id,
            call_id,
            call_type,
        )
        .await
        {
            tracing::error!(
                default_target_count = default_tokens.len(),
                error = %error,
                "Relay APNs call alert send failed"
            );
            errors.push(error.to_string());
        }
    }

    if !errors.is_empty() {
        return Err(AppError::Internal(anyhow::anyhow!(errors.join("; "))));
    }

    tracing::info!(
        voip_target_count = voip_tokens.len(),
        default_target_count = default_tokens.len(),
        recipient_id = %recipient_id,
        caller_id = %caller_id,
        call_id = %call_id,
        apns_use_sandbox = apns_cfg.use_sandbox,
        "Relay APNs incoming_call send success"
    );

    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/webhook", web::post().to(webhook));
}
