use chrono::{Duration, Utc};
use diesel::prelude::*;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::federation::{
    FederationActorKey, FederationDelivery, NewFederationActorKey, NewFederationDelivery,
    NewFederationInboxActivity, NewFederationRemoteMessage,
};
use crate::models::user::User;
use crate::schema::federation_actor_keys::dsl as key_dsl;
use crate::schema::federation_deliveries::dsl as delivery_dsl;
use crate::schema::federation_inbox_activities::dsl as inbox_dsl;
use crate::schema::federation_remote_messages::dsl as remote_msg_dsl;
use crate::schema::users::dsl as user_dsl;

pub fn ensure_actor_key(
    pool: &Pool,
    username: &str,
    key_id: &str,
    public_pem: &str,
    private_pkcs8_b64: &str,
) -> Result<FederationActorKey, AppError> {
    let mut conn = pool.get()?;

    let existing = key_dsl::federation_actor_keys
        .filter(key_dsl::actor_username.eq(username))
        .first::<FederationActorKey>(&mut conn)
        .optional()?;

    if let Some(found) = existing {
        return Ok(found);
    }

    diesel::insert_into(key_dsl::federation_actor_keys)
        .values(&NewFederationActorKey {
            id: Uuid::new_v4(),
            actor_username: username.to_string(),
            key_id: key_id.to_string(),
            public_key_pem: public_pem.to_string(),
            private_key_pkcs8: private_pkcs8_b64.to_string(),
        })
        .execute(&mut conn)?;

    key_dsl::federation_actor_keys
        .filter(key_dsl::actor_username.eq(username))
        .first::<FederationActorKey>(&mut conn)
        .map_err(AppError::from)
}

pub fn get_actor_key(pool: &Pool, username: &str) -> Result<Option<FederationActorKey>, AppError> {
    let mut conn = pool.get()?;
    key_dsl::federation_actor_keys
        .filter(key_dsl::actor_username.eq(username))
        .first::<FederationActorKey>(&mut conn)
        .optional()
        .map_err(AppError::from)
}

pub fn local_user_exists(pool: &Pool, username: &str) -> Result<bool, AppError> {
    let mut conn = pool.get()?;
    let found = user_dsl::users
        .filter(user_dsl::username.eq(username))
        .select(User::as_select())
        .first::<User>(&mut conn)
        .optional()?;
    Ok(found.is_some())
}

pub fn record_inbox_activity(
    pool: &Pool,
    activity_id: &str,
    actor: &str,
    recipient_username: &str,
    activity_type: &str,
    payload: serde_json::Value,
) -> Result<bool, AppError> {
    let mut conn = pool.get()?;

    let inserted = diesel::insert_into(inbox_dsl::federation_inbox_activities)
        .values(&NewFederationInboxActivity {
            id: Uuid::new_v4(),
            activity_id: activity_id.to_string(),
            actor: actor.to_string(),
            recipient_username: recipient_username.to_string(),
            activity_type: activity_type.to_string(),
            payload,
        })
        .on_conflict(inbox_dsl::activity_id)
        .do_nothing()
        .execute(&mut conn)?;

    Ok(inserted > 0)
}

pub fn upsert_delivery_pending(
    pool: &Pool,
    activity_id: &str,
    sender_username: &str,
    destination: &str,
) -> Result<FederationDelivery, AppError> {
    let mut conn = pool.get()?;

    diesel::insert_into(delivery_dsl::federation_deliveries)
        .values(&NewFederationDelivery {
            id: Uuid::new_v4(),
            activity_id: activity_id.to_string(),
            sender_username: sender_username.to_string(),
            destination: destination.to_string(),
            status: "pending".to_string(),
        })
        .on_conflict((delivery_dsl::activity_id, delivery_dsl::destination))
        .do_update()
        .set((
            delivery_dsl::status.eq("pending"),
            delivery_dsl::last_error.eq::<Option<String>>(None),
            delivery_dsl::next_attempt_at.eq::<Option<chrono::DateTime<Utc>>>(None),
        ))
        .execute(&mut conn)?;

    delivery_dsl::federation_deliveries
        .filter(delivery_dsl::activity_id.eq(activity_id))
        .filter(delivery_dsl::destination.eq(destination))
        .first::<FederationDelivery>(&mut conn)
        .map_err(AppError::from)
}

pub fn mark_delivery_success(pool: &Pool, delivery_id: Uuid) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    diesel::update(delivery_dsl::federation_deliveries.find(delivery_id))
        .set((
            delivery_dsl::status.eq("delivered"),
            delivery_dsl::attempts.eq(delivery_dsl::attempts + 1),
            delivery_dsl::delivered_at.eq(Some(Utc::now())),
            delivery_dsl::next_attempt_at.eq::<Option<chrono::DateTime<Utc>>>(None),
            delivery_dsl::last_error.eq::<Option<String>>(None),
        ))
        .execute(&mut conn)?;
    Ok(())
}

pub fn mark_delivery_failure(
    pool: &Pool,
    delivery_id: Uuid,
    error_message: &str,
    max_attempts: i32,
) -> Result<(), AppError> {
    let mut conn = pool.get()?;

    let delivery = delivery_dsl::federation_deliveries
        .find(delivery_id)
        .first::<FederationDelivery>(&mut conn)?;

    let next_attempt = delivery.attempts + 1;
    let status = if next_attempt >= max_attempts {
        "dead_letter"
    } else {
        "failed"
    };

    // Exponential backoff with deterministic jitter (seconds).
    let base_secs = 2_i64.pow(next_attempt.min(10) as u32);
    let jitter = (delivery_id.as_u128() % 17) as i64;
    let eta = Utc::now() + Duration::seconds(base_secs + jitter);

    diesel::update(delivery_dsl::federation_deliveries.find(delivery_id))
        .set((
            delivery_dsl::status.eq(status),
            delivery_dsl::attempts.eq(next_attempt),
            delivery_dsl::last_error.eq(Some(error_message.to_string())),
            delivery_dsl::next_attempt_at.eq(if status == "dead_letter" {
                None
            } else {
                Some(eta)
            }),
        ))
        .execute(&mut conn)?;

    Ok(())
}

pub fn deliveries_due_for_retry(pool: &Pool) -> Result<Vec<FederationDelivery>, AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();

    let rows = delivery_dsl::federation_deliveries
        .filter(
            delivery_dsl::status
                .eq("failed")
                .and(delivery_dsl::next_attempt_at.le(Some(now))),
        )
        .order(delivery_dsl::created_at.asc())
        .load::<FederationDelivery>(&mut conn)?;

    Ok(rows)
}

pub fn list_outbox_deliveries(
    pool: &Pool,
    username: &str,
    limit: i64,
) -> Result<Vec<FederationDelivery>, AppError> {
    let mut conn = pool.get()?;
    delivery_dsl::federation_deliveries
        .filter(delivery_dsl::sender_username.eq(username))
        .order(delivery_dsl::created_at.desc())
        .limit(limit)
        .load::<FederationDelivery>(&mut conn)
        .map_err(AppError::from)
}

pub fn map_create_note_to_local_message(
    pool: &Pool,
    activity_id: &str,
    activity_actor: &str,
    recipient_username: &str,
    payload: &serde_json::Value,
) -> Result<bool, AppError> {
    let object = match payload.get("object") {
        Some(v) => v,
        None => return Ok(false),
    };

    let object_type = object
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if object_type != "Note" {
        return Ok(false);
    }

    let content = object
        .get("content")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or_default();
    if content.is_empty() {
        return Ok(false);
    }

    let object_id = object
        .get("id")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    let mut conn = pool.get()?;
    let inserted = diesel::insert_into(remote_msg_dsl::federation_remote_messages)
        .values(&NewFederationRemoteMessage {
            id: Uuid::new_v4(),
            activity_id: activity_id.to_string(),
            object_id,
            actor: activity_actor.to_string(),
            recipient_username: recipient_username.to_string(),
            content: content.to_string(),
        })
        .on_conflict(remote_msg_dsl::activity_id)
        .do_nothing()
        .execute(&mut conn)?;

    Ok(inserted > 0)
}

pub fn parse_resource(resource: &str, expected_domain: &str) -> Result<String, AppError> {
    let rest = resource
        .strip_prefix("acct:")
        .ok_or_else(|| AppError::BadRequest("resource must start with acct:".into()))?;
    let (username, domain) = rest
        .split_once('@')
        .ok_or_else(|| AppError::BadRequest("resource must look like acct:user@domain".into()))?;

    if domain != expected_domain {
        return Err(AppError::NotFound);
    }

    let normalized = username.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(AppError::BadRequest("username cannot be empty".into()));
    }
    if !normalized
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' || c == '.')
    {
        return Err(AppError::BadRequest(
            "username contains unsupported characters".into(),
        ));
    }

    Ok(normalized)
}

pub fn permanent_failure_reason(status: u16) -> Option<&'static str> {
    if (400..500).contains(&status) && status != 429 {
        return Some("permanent remote 4xx response");
    }
    None
}

pub fn digest_sha256_base64(payload: &[u8]) -> String {
    use base64::Engine;
    use ring::digest::{digest, SHA256};

    let hash = digest(&SHA256, payload);
    base64::engine::general_purpose::STANDARD.encode(hash.as_ref())
}

pub fn parse_key_id_actor_url(key_id: &str) -> Result<String, AppError> {
    let (actor_url, _) = key_id
        .split_once('#')
        .ok_or_else(|| AppError::BadRequest("Signature keyId must include fragment".into()))?;

    if !(actor_url.starts_with("https://") || actor_url.starts_with("http://")) {
        return Err(AppError::BadRequest(
            "Signature keyId must be an absolute URL".into(),
        ));
    }

    Ok(actor_url.to_string())
}

pub fn ensure_activity_id(activity: &serde_json::Value) -> Result<String, AppError> {
    activity
        .get("id")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .ok_or_else(|| AppError::BadRequest("Activity must contain string id".into()))
}

pub fn ensure_activity_type(activity: &serde_json::Value) -> Result<String, AppError> {
    activity
        .get("type")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .ok_or_else(|| AppError::BadRequest("Activity must contain string type".into()))
}

pub fn ensure_activity_actor(activity: &serde_json::Value) -> Result<String, AppError> {
    activity
        .get("actor")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .ok_or_else(|| AppError::BadRequest("Activity must contain string actor".into()))
}

pub fn validate_activity_shape(activity: &serde_json::Value) -> Result<(), AppError> {
    let t = ensure_activity_type(activity)?;
    match t.as_str() {
        "Create" | "Follow" | "Accept" => Ok(()),
        _ => Err(AppError::BadRequest(format!(
            "Unsupported activity type: {t}"
        ))),
    }
}

pub fn ensure_local_actor_alignment(activity_actor: &str, actor_url: &str) -> Result<(), AppError> {
    if activity_actor != actor_url {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

pub fn max_payload_bytes() -> usize {
    256 * 1024
}

pub fn now_timestamp() -> i64 {
    Utc::now().timestamp()
}

pub fn parse_rfc2822_timestamp(value: &str) -> Result<i64, AppError> {
    chrono::DateTime::parse_from_rfc2822(value)
        .map(|d| d.timestamp())
        .map_err(|e| AppError::BadRequest(format!("Invalid Date header: {e}")))
}

pub fn validate_replay_window(ts: i64, now: i64, window_secs: i64) -> Result<(), AppError> {
    let drift = (now - ts).abs();
    if drift > window_secs {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

pub fn ensure_remote_domain_allowed(actor_url: &str, denylist: &[String]) -> Result<(), AppError> {
    let parsed = reqwest::Url::parse(actor_url)
        .map_err(|e| AppError::BadRequest(format!("Invalid actor URL: {e}")))?;

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::BadRequest("Actor URL host missing".into()))?;

    if denylist.iter().any(|d| d.eq_ignore_ascii_case(host)) {
        return Err(AppError::Forbidden);
    }

    Ok(())
}
