use actix_web::{http::header, web, HttpRequest, HttpResponse};
use base64::Engine;
use chrono::Utc;
use reqwest::StatusCode;
use ring::rand::SystemRandom;
use ring::signature::{self, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::RwLock;

use crate::auth::AuthUser;
use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::services::{federation_service, push_dispatch_service, redis_pubsub};

#[derive(Debug, Deserialize)]
pub struct WebFingerQuery {
    pub resource: String,
}

#[derive(Debug, Deserialize)]
pub struct SendFederatedMessageRequest {
    pub to_inboxes: Vec<String>,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct DeliveryResult {
    destination: String,
    status: String,
    attempts: i32,
    detail: Option<String>,
}

#[derive(Debug)]
struct SignatureFields {
    key_id: String,
    signature: String,
    headers: Vec<String>,
}

#[derive(Debug, Clone)]
struct CachedPublicKey {
    public_key_pem: String,
    fetched_at_ts: i64,
}

fn actor_key_cache() -> &'static RwLock<HashMap<String, CachedPublicKey>> {
    static KEY_CACHE: OnceLock<RwLock<HashMap<String, CachedPublicKey>>> = OnceLock::new();
    KEY_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn instance_base(domain: &str) -> String {
    format!("https://{domain}")
}

fn actor_url(domain: &str, username: &str) -> String {
    format!("{}/users/{username}", instance_base(domain))
}

fn key_id(domain: &str, username: &str) -> String {
    format!("{}#main-key", actor_url(domain, username))
}

fn build_remote_inbox_url(
    remote_server_url: &str,
    remote_user_id: &str,
) -> Result<String, AppError> {
    let mut parsed = reqwest::Url::parse(remote_server_url.trim())
        .map_err(|e| AppError::BadRequest(format!("Invalid recipient_server_url: {e}")))?;
    let normalized_user = remote_user_id.trim().to_lowercase();
    if normalized_user.is_empty() {
        return Err(AppError::BadRequest("recipient_id cannot be empty".into()));
    }

    let path = parsed.path().trim_end_matches('/');
    parsed.set_path(&format!("{path}/users/{normalized_user}/inbox"));
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

fn encode_raw_pem(label: &str, bytes: &[u8]) -> String {
    let body = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("-----BEGIN {label}-----\n{body}\n-----END {label}-----")
}

fn parse_raw_pem_bytes(pem: &str) -> Result<Vec<u8>, AppError> {
    let lines: Vec<&str> = pem.lines().collect();
    if lines.len() < 3 {
        return Err(AppError::BadRequest("Malformed PEM".into()));
    }
    let b64 = lines[1..lines.len() - 1].join("");
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| AppError::BadRequest(format!("Invalid PEM base64: {e}")))
}

fn parse_signature_header(sig: &str) -> Result<SignatureFields, AppError> {
    let mut key_id = None;
    let mut signature = None;
    let mut headers = None;

    for part in sig.split(',') {
        let (k, v) = part
            .trim()
            .split_once('=')
            .ok_or_else(|| AppError::BadRequest("Malformed Signature header".into()))?;
        let val = v.trim().trim_matches('"').to_string();
        match k.trim() {
            "keyId" => key_id = Some(val),
            "signature" => signature = Some(val),
            "headers" => {
                headers = Some(
                    val.split_whitespace()
                        .map(|s| s.trim().to_lowercase())
                        .collect::<Vec<_>>(),
                )
            }
            _ => {}
        }
    }

    let key_id = key_id.ok_or_else(|| AppError::BadRequest("Signature missing keyId".into()))?;
    let signature =
        signature.ok_or_else(|| AppError::BadRequest("Signature missing signature".into()))?;
    let headers =
        headers.ok_or_else(|| AppError::BadRequest("Signature missing headers list".into()))?;

    if !headers.iter().any(|h| h == "date") {
        return Err(AppError::BadRequest(
            "Signature headers must include date".into(),
        ));
    }

    Ok(SignatureFields {
        key_id,
        signature,
        headers,
    })
}

fn canonicalize_signed_string(req: &HttpRequest, headers: &[String]) -> Result<String, AppError> {
    let mut parts = Vec::with_capacity(headers.len());

    for h in headers {
        if h == "(request-target)" {
            let p = format!(
                "(request-target): {} {}",
                req.method().as_str().to_lowercase(),
                req.uri()
                    .path_and_query()
                    .map(|v| v.as_str())
                    .unwrap_or(req.uri().path())
            );
            parts.push(p);
            continue;
        }

        let value = req
            .headers()
            .get(h)
            .ok_or_else(|| AppError::BadRequest(format!("Missing signed header: {h}")))?
            .to_str()
            .map_err(|e| AppError::BadRequest(format!("Invalid header value for {h}: {e}")))?;

        parts.push(format!("{h}: {value}"));
    }

    Ok(parts.join("\n"))
}

fn sign_canonical(private_pkcs8_b64: &str, canonical: &str) -> Result<String, AppError> {
    let pkcs8 = base64::engine::general_purpose::STANDARD
        .decode(private_pkcs8_b64)
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Invalid stored private key base64: {e}"))
        })?;

    let pair = Ed25519KeyPair::from_pkcs8(&pkcs8)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Failed to parse Ed25519 private key")))?;
    let sig = pair.sign(canonical.as_bytes());
    Ok(base64::engine::general_purpose::STANDARD.encode(sig.as_ref()))
}

fn verify_signature(
    public_key_pem: &str,
    canonical: &str,
    signature_b64: &str,
) -> Result<(), AppError> {
    let public_key_raw = parse_raw_pem_bytes(public_key_pem)?;
    let signature_raw = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| AppError::BadRequest(format!("Invalid signature base64: {e}")))?;

    let verifier = UnparsedPublicKey::new(&signature::ED25519, public_key_raw);
    verifier
        .verify(canonical.as_bytes(), &signature_raw)
        .map_err(|_| AppError::Unauthorized)
}

fn build_signature_header(
    req: &reqwest::Request,
    key_id: &str,
    private_pkcs8_b64: &str,
) -> Result<String, AppError> {
    let path = req
        .url()
        .query()
        .map(|q| format!("{}?{q}", req.url().path()))
        .unwrap_or_else(|| req.url().path().to_string());
    let host = req
        .url()
        .host_str()
        .ok_or_else(|| AppError::BadRequest("Destination URL missing host".into()))?;

    let date = req
        .headers()
        .get("date")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::BadRequest("Missing date header".into()))?;
    let digest = req
        .headers()
        .get("digest")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::BadRequest("Missing digest header".into()))?;

    let signed_headers = ["(request-target)", "host", "date", "digest"];
    let canonical = format!(
        "(request-target): {} {}\nhost: {}\ndate: {}\ndigest: {}",
        req.method().as_str().to_lowercase(),
        &path,
        host,
        date,
        digest
    );

    let sig_b64 = sign_canonical(private_pkcs8_b64, &canonical)?;
    Ok(format!(
        "keyId=\"{}\",algorithm=\"ed25519\",headers=\"{}\",signature=\"{}\"",
        key_id,
        signed_headers.join(" "),
        sig_b64
    ))
}

async fn fetch_actor_public_key(
    actor_url: &str,
    cfg: &Config,
    force_refresh: bool,
) -> Result<String, AppError> {
    if !force_refresh {
        let now = Utc::now().timestamp();
        let cache = actor_key_cache().read().await;
        if let Some(cached) = cache.get(actor_url) {
            let age = now - cached.fetched_at_ts;
            if age <= cfg.federation_key_cache_ttl_secs {
                return Ok(cached.public_key_pem.clone());
            }
        }
    }

    let resp = reqwest::Client::new()
        .get(actor_url)
        .header("accept", "application/activity+json")
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch actor document: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Unauthorized);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Invalid actor JSON: {e}")))?;

    let public_key = body
        .get("publicKey")
        .and_then(|v| v.get("publicKeyPem"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .ok_or(AppError::Unauthorized)?;

    let mut cache = actor_key_cache().write().await;
    cache.insert(
        actor_url.to_string(),
        CachedPublicKey {
            public_key_pem: public_key.clone(),
            fetched_at_ts: Utc::now().timestamp(),
        },
    );

    Ok(public_key)
}

fn ensure_actor_key_material(
    pool: &Pool,
    cfg: &Config,
    username: &str,
) -> Result<crate::models::federation::FederationActorKey, AppError> {
    if let Some(found) = federation_service::get_actor_key(pool, username)? {
        return Ok(found);
    }

    let rng = SystemRandom::new();
    let generated = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Failed to generate Ed25519 keypair")))?;

    let pair = Ed25519KeyPair::from_pkcs8(generated.as_ref())
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Failed to parse generated keypair")))?;

    let public_pem = encode_raw_pem("PUBLIC KEY", pair.public_key().as_ref());
    let private_pkcs8_b64 = base64::engine::general_purpose::STANDARD.encode(generated.as_ref());

    federation_service::ensure_actor_key(
        pool,
        username,
        &key_id(&cfg.instance_domain, username),
        &public_pem,
        &private_pkcs8_b64,
    )
}

pub async fn webfinger(
    cfg: web::Data<Config>,
    pool: web::Data<Pool>,
    q: web::Query<WebFingerQuery>,
) -> Result<HttpResponse, AppError> {
    let username = federation_service::parse_resource(&q.resource, &cfg.instance_domain)?;
    if !federation_service::local_user_exists(&pool, &username)? {
        return Err(AppError::NotFound);
    }

    let actor = actor_url(&cfg.instance_domain, &username);

    Ok(HttpResponse::Ok()
        .content_type("application/jrd+json")
        .json(json!({
            "subject": format!("acct:{username}@{}", cfg.instance_domain),
            "links": [
                {
                    "rel": "self",
                    "type": "application/activity+json",
                    "href": actor,
                }
            ]
        })))
}

pub async fn actor_profile(
    cfg: web::Data<Config>,
    pool: web::Data<Pool>,
    username: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let username = username.into_inner().to_lowercase();
    if !federation_service::local_user_exists(&pool, &username)? {
        return Err(AppError::NotFound);
    }

    let key = ensure_actor_key_material(&pool, &cfg, &username)?;
    let id = actor_url(&cfg.instance_domain, &username);

    Ok(HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "application/activity+json"))
        .json(json!({
            "@context": ["https://www.w3.org/ns/activitystreams", "https://w3id.org/security/v1"],
            "id": id,
            "type": "Person",
            "preferredUsername": username,
            "inbox": format!("{}/inbox", id),
            "outbox": format!("{}/outbox", id),
            "publicKey": {
                "id": key.key_id,
                "owner": id,
                "publicKeyPem": key.public_key_pem,
            }
        })))
}

pub async fn outbox(
    pool: web::Data<Pool>,
    username: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let username = username.into_inner().to_lowercase();
    let items = federation_service::list_outbox_deliveries(&pool, &username, 100)?;

    Ok(HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "application/activity+json"))
        .json(json!({
            "type": "OrderedCollection",
            "totalItems": items.len(),
            "orderedItems": items,
        })))
}

pub async fn inbox(
    req: HttpRequest,
    cfg: web::Data<Config>,
    pool: web::Data<Pool>,
    redis: web::Data<redis::Client>,
    username: web::Path<String>,
    body: web::Bytes,
) -> Result<HttpResponse, AppError> {
    if body.len() > federation_service::max_payload_bytes() {
        return Err(AppError::BadRequest("Payload too large".into()));
    }

    let username = username.into_inner().to_lowercase();
    if !federation_service::local_user_exists(&pool, &username)? {
        return Err(AppError::NotFound);
    }

    let digest_header = req
        .headers()
        .get("digest")
        .ok_or_else(|| AppError::BadRequest("Missing Digest header".into()))?
        .to_str()
        .map_err(|e| AppError::BadRequest(format!("Invalid Digest header: {e}")))?;

    let expected_digest = format!(
        "SHA-256={}",
        federation_service::digest_sha256_base64(&body)
    );
    if digest_header != expected_digest {
        return Err(AppError::Unauthorized);
    }

    let date_header = req
        .headers()
        .get("date")
        .ok_or_else(|| AppError::BadRequest("Missing Date header".into()))?
        .to_str()
        .map_err(|e| AppError::BadRequest(format!("Invalid Date header: {e}")))?;

    let request_ts = federation_service::parse_rfc2822_timestamp(date_header)?;
    federation_service::validate_replay_window(
        request_ts,
        federation_service::now_timestamp(),
        cfg.federation_signature_window_secs,
    )?;

    let sig_header = req
        .headers()
        .get("signature")
        .ok_or_else(|| AppError::BadRequest("Missing Signature header".into()))?
        .to_str()
        .map_err(|e| AppError::BadRequest(format!("Invalid Signature header: {e}")))?;

    let sig_fields = parse_signature_header(sig_header)?;
    let actor_url = federation_service::parse_key_id_actor_url(&sig_fields.key_id)?;
    federation_service::ensure_remote_domain_allowed(&actor_url, &cfg.federation_denylist)?;

    let canonical = canonicalize_signed_string(&req, &sig_fields.headers)?;
    let public_key_pem = fetch_actor_public_key(&actor_url, &cfg, false).await?;
    if verify_signature(&public_key_pem, &canonical, &sig_fields.signature).is_err() {
        let mut cache = actor_key_cache().write().await;
        cache.remove(&actor_url);
        drop(cache);

        let refreshed_key = fetch_actor_public_key(&actor_url, &cfg, true).await?;
        verify_signature(&refreshed_key, &canonical, &sig_fields.signature)?;
    }

    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON body: {e}")))?;

    federation_service::validate_activity_shape(&payload)?;
    let activity_id = federation_service::ensure_activity_id(&payload)?;
    let activity_type = federation_service::ensure_activity_type(&payload)?;
    let activity_actor = federation_service::ensure_activity_actor(&payload)?;
    federation_service::ensure_local_actor_alignment(&activity_actor, &actor_url)?;

    let inserted = federation_service::record_inbox_activity(
        &pool,
        &activity_id,
        &activity_actor,
        &username,
        &activity_type,
        payload.clone(),
    )?;

    if !inserted {
        return Ok(HttpResponse::Accepted().json(json!({
            "status": "duplicate",
            "activity_id": activity_id,
        })));
    }

    if activity_type == "Create" {
        let maybe_message = federation_service::map_create_note_to_local_message(
            &pool,
            &activity_id,
            &activity_actor,
            &username,
            &payload,
        )?;

        if let Some(message) = maybe_message {
            let event = serde_json::json!({ "type": "new_message", "message": &message });
            if let Ok(mut conn) = redis_pubsub::get_async_conn(&redis).await {
                let _ = redis_pubsub::publish(
                    &mut conn,
                    &redis_pubsub::user_channel(message.recipient_id),
                    &event,
                )
                .await;
            }

            let push_pool = pool.get_ref().clone();
            let push_cfg = cfg.get_ref().clone();
            let push_sender_id = message.sender_id;
            let push_recipient_id = message.recipient_id;
            let push_message_id = message.id;
            let push_content = message.content.clone();
            actix_web::rt::spawn(async move {
                if let Err(error) = push_dispatch_service::dispatch_new_message(
                    &push_pool,
                    &push_cfg,
                    push_recipient_id,
                    push_sender_id,
                    push_message_id,
                    &push_content,
                )
                .await
                {
                    match &error {
                        AppError::Internal(cause) => {
                            tracing::warn!(
                                error = %error,
                                cause = %cause,
                                error_debug = ?error,
                                "Push dispatch failed for inbound federation message"
                            );
                        }
                        _ => {
                            tracing::warn!(
                                error = %error,
                                error_debug = ?error,
                                "Push dispatch failed for inbound federation message"
                            );
                        }
                    }
                }
            });
        }
    }

    Ok(HttpResponse::Accepted().json(json!({
        "status": "accepted",
        "activity_id": activity_id,
    })))
}

async fn post_activity_signed(
    destination: &str,
    key_id_value: &str,
    private_pkcs8_b64: &str,
    payload_bytes: &[u8],
) -> Result<StatusCode, AppError> {
    let client = reqwest::Client::new();
    let digest = format!(
        "SHA-256={}",
        federation_service::digest_sha256_base64(payload_bytes)
    );
    let date = Utc::now().to_rfc2822();

    let mut req = client
        .post(destination)
        .header("accept", "application/activity+json")
        .header("content-type", "application/activity+json")
        .header("date", &date)
        .header("digest", &digest)
        .body(payload_bytes.to_vec())
        .build()
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to build outbound request: {e}"))
        })?;

    let signature = build_signature_header(&req, key_id_value, private_pkcs8_b64)?;
    req.headers_mut().insert(
        "signature",
        reqwest::header::HeaderValue::from_str(&signature)
            .map_err(|e| AppError::BadRequest(format!("Invalid signature header value: {e}")))?,
    );

    let response = client
        .execute(req)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Outbound federation call failed: {e}")))?;

    Ok(response.status())
}

pub async fn send_to_remote(
    cfg: web::Data<Config>,
    pool: web::Data<Pool>,
    auth: AuthUser,
    body: web::Json<SendFederatedMessageRequest>,
) -> Result<HttpResponse, AppError> {
    if body.to_inboxes.is_empty() {
        return Err(AppError::BadRequest("to_inboxes cannot be empty".into()));
    }
    if body.content.trim().is_empty() {
        return Err(AppError::BadRequest("content cannot be empty".into()));
    }

    let claims = auth.0;
    let sender_id = claims.user_id()?;

    let mut conn = pool.get()?;
    use crate::schema::users::dsl as user_dsl;
    use diesel::prelude::*;
    let sender_username = user_dsl::users
        .filter(user_dsl::id.eq(sender_id))
        .select(user_dsl::username)
        .first::<String>(&mut conn)?;

    let key = ensure_actor_key_material(&pool, &cfg, &sender_username)?;

    let activity_id = format!(
        "{}/activities/{}",
        actor_url(&cfg.instance_domain, &sender_username),
        uuid::Uuid::new_v4()
    );
    let actor = actor_url(&cfg.instance_domain, &sender_username);

    let payload = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": activity_id,
        "type": "Create",
        "actor": actor,
        "object": {
            "id": format!("{}/objects/{}", actor_url(&cfg.instance_domain, &sender_username), uuid::Uuid::new_v4()),
            "type": "Note",
            "content": body.content,
        },
        "to": body.to_inboxes,
    });
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize activity: {e}")))?;

    let mut results = Vec::with_capacity(body.to_inboxes.len());

    for destination in &body.to_inboxes {
        let started = std::time::Instant::now();
        let destination_host = reqwest::Url::parse(destination)
            .ok()
            .and_then(|u| u.host_str().map(ToString::to_string))
            .unwrap_or_else(|| "unknown".to_string());

        let delivery = federation_service::upsert_delivery_pending(
            &pool,
            &activity_id,
            &sender_username,
            destination,
        )?;

        match post_activity_signed(
            destination,
            &key.key_id,
            &key.private_key_pkcs8,
            &payload_bytes,
        )
        .await
        {
            Ok(status) if status.is_success() => {
                federation_service::mark_delivery_success(&pool, delivery.id)?;
                tracing::info!(
                    destination = %destination,
                    destination_host = %destination_host,
                    status = %status.as_u16(),
                    retry_count = delivery.attempts + 1,
                    latency_ms = started.elapsed().as_millis(),
                    "federation delivery succeeded"
                );
                results.push(DeliveryResult {
                    destination: destination.clone(),
                    status: "delivered".into(),
                    attempts: delivery.attempts + 1,
                    detail: None,
                });
            }
            Ok(status) => {
                let status_u16 = status.as_u16();
                let permanent = federation_service::permanent_failure_reason(status_u16).is_some();
                federation_service::mark_delivery_failure(
                    &pool,
                    delivery.id,
                    &format!("remote status {status_u16}"),
                    if permanent {
                        1
                    } else {
                        cfg.federation_max_delivery_attempts
                    },
                )?;
                tracing::warn!(
                    destination = %destination,
                    destination_host = %destination_host,
                    status = %status_u16,
                    retry_count = delivery.attempts + 1,
                    latency_ms = started.elapsed().as_millis(),
                    permanent_failure = permanent,
                    "federation delivery failed with remote status"
                );
                results.push(DeliveryResult {
                    destination: destination.clone(),
                    status: if permanent {
                        "dead_letter".into()
                    } else {
                        "failed".into()
                    },
                    attempts: delivery.attempts + 1,
                    detail: Some(format!("remote status {status_u16}")),
                });
            }
            Err(err) => {
                federation_service::mark_delivery_failure(
                    &pool,
                    delivery.id,
                    &format!("transport error: {err}"),
                    cfg.federation_max_delivery_attempts,
                )?;
                tracing::warn!(
                    destination = %destination,
                    destination_host = %destination_host,
                    status = "transport_error",
                    retry_count = delivery.attempts + 1,
                    latency_ms = started.elapsed().as_millis(),
                    error = %err,
                    "federation delivery failed with transport error"
                );
                results.push(DeliveryResult {
                    destination: destination.clone(),
                    status: "failed".into(),
                    attempts: delivery.attempts + 1,
                    detail: Some(format!("transport error: {err}")),
                });
            }
        }
    }

    Ok(HttpResponse::Accepted().json(json!({
        "activity_id": activity_id,
        "results": results,
    })))
}

pub async fn deliver_direct_message_to_remote(
    cfg: &Config,
    pool: &Pool,
    sender_id: uuid::Uuid,
    recipient_id: &str,
    recipient_server_url: &str,
    content: &str,
) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    use crate::schema::users::dsl as user_dsl;
    use diesel::prelude::*;

    let sender_username = user_dsl::users
        .filter(user_dsl::id.eq(sender_id))
        .select(user_dsl::username)
        .first::<String>(&mut conn)?;

    let key = ensure_actor_key_material(pool, cfg, &sender_username)?;
    let destination = build_remote_inbox_url(recipient_server_url, recipient_id)?;
    let destination_host = reqwest::Url::parse(&destination)
        .ok()
        .and_then(|u| u.host_str().map(ToString::to_string))
        .unwrap_or_else(|| "unknown".to_string());

    let activity_id = format!(
        "{}/activities/{}",
        actor_url(&cfg.instance_domain, &sender_username),
        uuid::Uuid::new_v4()
    );
    let sender_actor = actor_url(&cfg.instance_domain, &sender_username);
    let payload = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": activity_id,
        "type": "Create",
        "actor": sender_actor,
        "object": {
            "id": format!("{}/objects/{}", actor_url(&cfg.instance_domain, &sender_username), uuid::Uuid::new_v4()),
            "type": "Note",
            "content": content,
        },
        "to": [destination.clone()],
    });
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize activity: {e}")))?;

    let delivery = federation_service::upsert_delivery_pending(
        pool,
        &activity_id,
        &sender_username,
        &destination,
    )?;

    let started = std::time::Instant::now();
    match post_activity_signed(
        &destination,
        &key.key_id,
        &key.private_key_pkcs8,
        &payload_bytes,
    )
    .await
    {
        Ok(status) if status.is_success() => {
            federation_service::mark_delivery_success(pool, delivery.id)?;
            tracing::info!(
                destination = %destination,
                destination_host = %destination_host,
                status = %status.as_u16(),
                retry_count = delivery.attempts + 1,
                latency_ms = started.elapsed().as_millis(),
                "federation delivery succeeded"
            );
            Ok(())
        }
        Ok(status) => {
            let status_u16 = status.as_u16();
            let permanent = federation_service::permanent_failure_reason(status_u16).is_some();
            federation_service::mark_delivery_failure(
                pool,
                delivery.id,
                &format!("remote status {status_u16}"),
                if permanent {
                    1
                } else {
                    cfg.federation_max_delivery_attempts
                },
            )?;
            Err(AppError::BadRequest(format!(
                "Remote server rejected message with status {status_u16}"
            )))
        }
        Err(err) => {
            federation_service::mark_delivery_failure(
                pool,
                delivery.id,
                &format!("transport error: {err}"),
                cfg.federation_max_delivery_attempts,
            )?;
            Err(err)
        }
    }
}

pub async fn retry_due_deliveries(
    cfg: web::Data<Config>,
    pool: web::Data<Pool>,
    _admin: crate::auth::AdminUser,
) -> Result<HttpResponse, AppError> {
    let due = federation_service::deliveries_due_for_retry(&pool)?;
    let mut retried = 0usize;

    for d in due {
        let key = match federation_service::get_actor_key(&pool, &d.sender_username)? {
            Some(k) => k,
            None => {
                federation_service::mark_delivery_failure(
                    &pool,
                    d.id,
                    "missing local actor key",
                    cfg.federation_max_delivery_attempts,
                )?;
                continue;
            }
        };

        // Retry payload cannot be reconstructed from DB in this minimal implementation.
        // We still advance state to dead-letter once max attempts is exceeded.
        federation_service::mark_delivery_failure(
            &pool,
            d.id,
            &format!("retry payload unavailable for {}", key.key_id),
            cfg.federation_max_delivery_attempts,
        )?;
        retried += 1;
    }

    Ok(HttpResponse::Ok().json(json!({ "retried": retried })))
}

pub fn configure_public(cfg: &mut web::ServiceConfig) {
    cfg.route("/.well-known/webfinger", web::get().to(webfinger))
        .route("/users/{username}", web::get().to(actor_profile))
        .route("/users/{username}/outbox", web::get().to(outbox))
        .route("/federation/send", web::post().to(send_to_remote))
        .route(
            "/federation/retry-due",
            web::post().to(retry_due_deliveries),
        );
}

pub fn configure_inbox(cfg: &mut web::ServiceConfig) {
    cfg.route("/users/{username}/inbox", web::post().to(inbox));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_signature_header_extracts_fields() {
        let sig = "keyId=\"https://a.example/users/alice#main-key\",algorithm=\"ed25519\",headers=\"(request-target) host date digest\",signature=\"abc123\"";
        let fields = parse_signature_header(sig).expect("must parse");
        assert_eq!(fields.key_id, "https://a.example/users/alice#main-key");
        assert_eq!(fields.signature, "abc123");
        assert_eq!(fields.headers[0], "(request-target)");
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let rng = SystemRandom::new();
        let generated = Ed25519KeyPair::generate_pkcs8(&rng).expect("keygen");
        let pair = Ed25519KeyPair::from_pkcs8(generated.as_ref()).expect("pair");

        let private_b64 = base64::engine::general_purpose::STANDARD.encode(generated.as_ref());
        let public_pem = encode_raw_pem("PUBLIC KEY", pair.public_key().as_ref());

        let canonical = "(request-target): post /users/bob/inbox\nhost: example.com\ndate: Wed, 04 Mar 2026 10:00:00 +0000\ndigest: SHA-256=abc";
        let signature = sign_canonical(&private_b64, canonical).expect("sign");
        verify_signature(&public_pem, canonical, &signature).expect("verify");
    }
}
