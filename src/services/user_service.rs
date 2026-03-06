use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;

use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::user::{NewUser, User};
use crate::schema::users::dsl::*;
use crate::services::admin_service;

const FEDERATED_SHADOW_EMAIL_DOMAIN: &str = "federated.sync.invalid";
const FEDERATED_DISABLED_PASSWORD_HASH: &str =
    "$2b$12$rMj14yi8T5IeDV.9xR4Lxehx2Y8M7vY4QfPh6UfZIe8TVfEfHE7Ni";

#[derive(Debug, Clone)]
pub struct FederatedIdentity {
    pub remote_user_id: String,
    pub remote_host: String,
}

fn normalize_remote_user_id(raw: &str) -> Result<String, AppError> {
    let normalized = raw.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(AppError::BadRequest("recipient_id cannot be empty".into()));
    }
    if !normalized
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-')
    {
        return Err(AppError::BadRequest(
            "recipient_id contains unsupported characters".into(),
        ));
    }
    Ok(normalized)
}

fn normalize_remote_host(server_url: &str) -> Result<String, AppError> {
    let parsed = reqwest::Url::parse(server_url.trim())
        .map_err(|e| AppError::BadRequest(format!("Invalid recipient_server_url: {e}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::BadRequest("recipient_server_url must include host".into()))?;
    Ok(host.to_lowercase())
}

fn federated_shadow_username(remote_user_id: &str, remote_host: &str) -> String {
    format!("{remote_user_id}@{remote_host}")
}

fn federated_shadow_email(username_value: &str) -> String {
    use ring::digest::{digest, SHA256};
    let hash = hex::encode(digest(&SHA256, username_value.as_bytes()).as_ref());
    format!("fed+{}@{}", &hash[..32], FEDERATED_SHADOW_EMAIL_DOMAIN)
}

/// Parse a federated shadow username (`user@host`) and return its identity.
pub fn parse_federated_shadow_username(username_value: &str) -> Option<FederatedIdentity> {
    let (remote_user_id, remote_host) = username_value.split_once('@')?;
    if remote_user_id.is_empty() || remote_host.is_empty() {
        return None;
    }
    if !remote_user_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-')
    {
        return None;
    }
    if !remote_host
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
    {
        return None;
    }

    Some(FederatedIdentity {
        remote_user_id: remote_user_id.to_string(),
        remote_host: remote_host.to_string(),
    })
}

/// Return federated identity when this user is a local shadow contact.
pub fn federated_identity_for_user(user: &User) -> Option<FederatedIdentity> {
    if !user.email.ends_with(FEDERATED_SHADOW_EMAIL_DOMAIN) {
        return None;
    }
    parse_federated_shadow_username(&user.username)
}

/// Look up a user by email. Returns `None` if not found.
pub fn find_by_email(pool: &Pool, user_email: &str) -> Result<Option<User>, AppError> {
    let mut conn = pool.get()?;
    let result = users
        .filter(email.eq(user_email))
        .filter(is_active.eq(true))
        .first::<User>(&mut conn)
        .optional()?;
    Ok(result)
}

/// Look up a user by primary key. Returns `None` if not found.
pub fn find_by_id(pool: &Pool, user_id: Uuid) -> Result<Option<User>, AppError> {
    let mut conn = pool.get()?;
    let result = users.find(user_id).first::<User>(&mut conn).optional()?;
    Ok(result)
}

pub fn find_by_username(pool: &Pool, lookup_username: &str) -> Result<Option<User>, AppError> {
    let mut conn = pool.get()?;
    users
        .filter(username.eq(lookup_username))
        .first::<User>(&mut conn)
        .optional()
        .map_err(AppError::from)
}

/// Look up a user by device auth public key.
pub fn find_by_device_auth_pubkey(
    pool: &Pool,
    pubkey_value: &str,
) -> Result<Option<User>, AppError> {
    let mut conn = pool.get()?;
    users
        .filter(device_auth_pubkey.eq(pubkey_value))
        .first::<User>(&mut conn)
        .optional()
        .map_err(AppError::from)
}

pub fn ensure_federated_shadow_user(
    pool: &Pool,
    remote_user_id: &str,
    remote_server_url: &str,
) -> Result<User, AppError> {
    let normalized_remote_user_id = normalize_remote_user_id(remote_user_id)?;
    let normalized_host = normalize_remote_host(remote_server_url)?;
    let shadow_username = federated_shadow_username(&normalized_remote_user_id, &normalized_host);

    if let Some(existing) = find_by_username(pool, &shadow_username)? {
        return Ok(existing);
    }

    let mut conn = pool.get()?;
    let email_value = federated_shadow_email(&shadow_username);
    let new_user = NewUser {
        id: Uuid::new_v4(),
        username: shadow_username.clone(),
        email: email_value,
        password_hash: FEDERATED_DISABLED_PASSWORD_HASH.to_string(),
        role: "user".to_string(),
        device_auth_pubkey: None,
        is_approved: true, // federated shadow accounts bypass approval gate
    };

    match diesel::insert_into(crate::schema::users::table)
        .values(&new_user)
        .get_result::<User>(&mut conn)
    {
        Ok(inserted) => {
            // Shadow contacts are never directly authenticatable accounts.
            let updated = diesel::update(users.find(inserted.id))
                .set(is_active.eq(false))
                .get_result::<User>(&mut conn)?;
            Ok(updated)
        }
        Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        )) => find_by_username(pool, &shadow_username)?.ok_or(AppError::NotFound),
        Err(other) => Err(AppError::Database(other)),
    }
}

/// Create a new user, enforcing the `MAX_USERS` limit when set.
///
/// Returns `AppError::Conflict` if email or username is already taken.
/// Returns `AppError::Conflict` if the instance has reached its user cap.
pub fn create_user(
    pool: &Pool,
    new_user: NewUser,
    max_users: Option<u32>,
) -> Result<User, AppError> {
    let mut conn = pool.get()?;

    // Enforce MAX_USERS cap
    if let Some(cap) = max_users {
        let count: i64 = users.count().get_result(&mut conn)?;
        if count >= cap as i64 {
            return Err(AppError::Conflict(format!(
                "Instance user limit reached (max_users={})",
                cap
            )));
        }
    }

    diesel::insert_into(crate::schema::users::table)
        .values(&new_user)
        .get_result::<User>(&mut conn)
        .map_err(|e| match e {
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                _,
            ) => AppError::Conflict("Email or username already taken".into()),
            other => AppError::Database(other),
        })
}

pub fn resolved_max_users(pool: &Pool, config: &Config) -> Result<Option<u32>, AppError> {
    admin_service::effective_max_users(pool, config)
}

/// Update `last_seen_at` for a user to now.
pub fn update_last_seen(pool: &Pool, user_id: Uuid) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    diesel::update(users.find(user_id))
        .set(last_seen_at.eq(Some(Utc::now())))
        .execute(&mut conn)?;
    Ok(())
}

pub fn update_profile(
    pool: &Pool,
    user_id: Uuid,
    next_username: Option<String>,
    next_avatar_base64: Option<Option<String>>,
    next_message_public_key: Option<Option<String>>,
) -> Result<User, AppError> {
    let mut conn = pool.get()?;
    let existing = users.find(user_id).first::<User>(&mut conn).optional()?;
    let existing = existing.ok_or(AppError::NotFound)?;

    let username_value = next_username.unwrap_or(existing.username);
    let avatar_value = match next_avatar_base64 {
        Some(value) => value,
        None => existing.avatar_base64,
    };
    let message_public_key_value = match next_message_public_key {
        Some(value) => value,
        None => existing.message_public_key,
    };

    diesel::update(users.find(user_id))
        .set((
            username.eq(username_value),
            avatar_base64.eq(avatar_value),
            message_public_key.eq(message_public_key_value),
        ))
        .get_result::<User>(&mut conn)
        .map_err(|e| match e {
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                _,
            ) => AppError::Conflict("Username already taken".into()),
            other => AppError::Database(other),
        })
}
