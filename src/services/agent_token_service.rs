use chrono::{DateTime, Duration, Utc};
use diesel::prelude::*;
use uuid::Uuid;

use crate::auth::tokens::{generate_refresh_token, hash_token};
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::{AgentToken, NewAgentToken};
use crate::schema::agent_tokens::dsl as tokens_dsl;

#[allow(dead_code)]
const MAX_NAME_CHARS: usize = 120;
#[allow(dead_code)]
const TOKEN_PREFIX_RAW_CHARS: usize = 8;

#[allow(dead_code)]
fn normalize_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("name cannot be empty".into()));
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::BadRequest(format!(
            "name must be <= {MAX_NAME_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

#[allow(dead_code)]
fn token_prefix(raw_token: &str) -> String {
    let without_prefix = raw_token.strip_prefix("agt_").unwrap_or(raw_token);
    let visible: String = without_prefix
        .chars()
        .take(TOKEN_PREFIX_RAW_CHARS)
        .collect();
    format!("agt_{visible}")
}

#[allow(dead_code)] // Used by tasks 4 and 5
pub fn compute_expires_at(
    now: DateTime<Utc>,
    expires_in_days: Option<i64>,
) -> Result<Option<DateTime<Utc>>, AppError> {
    match expires_in_days {
        None => Ok(None),
        Some(days) if days <= 0 => Err(AppError::BadRequest(
            "expires_in_days must be a positive number of days".into(),
        )),
        Some(days) => Ok(Some(now + Duration::days(days))),
    }
}

/// Generates a fresh `agt_`-prefixed raw token and the hash `verify()` will
/// later look it up by. Kept as its own function so a test can assert the
/// invariant directly against production code, rather than re-deriving the
/// hash inline (which would pass even if `create()` regressed to hashing
/// the unprefixed generator output).
fn issue_raw_token() -> (String, String) {
    let (raw_suffix, _) = generate_refresh_token();
    let raw_token = format!("agt_{raw_suffix}");
    let hash = hash_token(&raw_token);
    (raw_token, hash)
}

/// Generates a new agent token, stores its hash, and returns the row plus
/// the raw secret. The raw secret is not recoverable after this call returns.
pub fn create(
    pool: &Pool,
    created_by: Uuid,
    name: &str,
    expires_in_days: Option<i64>,
) -> Result<(AgentToken, String), AppError> {
    let normalized_name = normalize_name(name)?;
    let expires_at = compute_expires_at(Utc::now(), expires_in_days)?;

    let (raw_token, hash) = issue_raw_token();

    let mut conn = pool.get()?;
    let payload = NewAgentToken {
        id: Uuid::new_v4(),
        name: normalized_name,
        token_hash: hash,
        token_prefix: token_prefix(&raw_token),
        created_by,
        expires_at,
    };

    diesel::insert_into(tokens_dsl::agent_tokens)
        .values(&payload)
        .execute(&mut conn)?;

    let row = tokens_dsl::agent_tokens
        .find(payload.id)
        .select(AgentToken::as_select())
        .first::<AgentToken>(&mut conn)
        .map_err(AppError::from)?;

    Ok((row, raw_token))
}

pub fn list(pool: &Pool) -> Result<Vec<AgentToken>, AppError> {
    let mut conn = pool.get()?;
    tokens_dsl::agent_tokens
        .order(tokens_dsl::created_at.desc())
        .select(AgentToken::as_select())
        .load::<AgentToken>(&mut conn)
        .map_err(AppError::from)
}

/// Idempotent: revoking an already-revoked (or nonexistent) token is not an error.
pub fn revoke(pool: &Pool, id: Uuid) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    diesel::update(tokens_dsl::agent_tokens.find(id))
        .filter(tokens_dsl::revoked_at.is_null())
        .set(tokens_dsl::revoked_at.eq(Some(Utc::now())))
        .execute(&mut conn)?;
    Ok(())
}

/// Looks up a presented raw token by its hash. Returns `None` if the token
/// is unknown, revoked, or past its expiry. On a valid match, best-effort
/// updates `last_used_at` — a failure to update must not fail the check.
pub fn verify(pool: &Pool, raw_token: &str) -> Result<Option<AgentToken>, AppError> {
    let hash = hash_token(raw_token);
    let mut conn = pool.get()?;

    let found = tokens_dsl::agent_tokens
        .filter(tokens_dsl::token_hash.eq(&hash))
        .select(AgentToken::as_select())
        .first::<AgentToken>(&mut conn)
        .optional()
        .map_err(AppError::from)?;

    let Some(token) = found else {
        return Ok(None);
    };

    if token.revoked_at.is_some() {
        return Ok(None);
    }
    if let Some(expires_at) = token.expires_at {
        if expires_at <= Utc::now() {
            return Ok(None);
        }
    }

    let _ = diesel::update(tokens_dsl::agent_tokens.find(token.id))
        .set(tokens_dsl::last_used_at.eq(Some(Utc::now())))
        .execute(&mut conn);

    Ok(Some(token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 15, 12, 0, 0).unwrap()
    }

    #[test]
    fn compute_expires_at_none_means_never_expires() {
        let result = compute_expires_at(fixed_now(), None).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn compute_expires_at_adds_days() {
        let result = compute_expires_at(fixed_now(), Some(30)).unwrap().unwrap();
        assert_eq!(result, fixed_now() + chrono::Duration::days(30));
    }

    #[test]
    fn compute_expires_at_rejects_non_positive_days() {
        assert!(compute_expires_at(fixed_now(), Some(0)).is_err());
        assert!(compute_expires_at(fixed_now(), Some(-5)).is_err());
    }

    #[test]
    fn normalize_name_trims_and_rejects_empty() {
        assert_eq!(
            normalize_name("  Claude ops agent  ").unwrap(),
            "Claude ops agent"
        );
        assert!(normalize_name("   ").is_err());
        assert!(normalize_name(&"a".repeat(121)).is_err());
    }

    #[test]
    fn token_prefix_is_stable_and_short() {
        let prefix = token_prefix("agt_abcdef0123456789");
        assert_eq!(prefix, "agt_abcdef01");
    }

    #[test]
    fn issue_raw_token_hash_matches_what_verify_would_compute() {
        // Exercises the actual function create() calls — not a re-derivation —
        // so this fails if create() ever regresses to hashing the unprefixed
        // generator output instead of the full "agt_"-prefixed bearer string
        // a client actually presents (the bug this test guards against).
        let (raw_token, stored_hash) = issue_raw_token();
        assert!(raw_token.starts_with("agt_"));
        let verify_lookup_hash = hash_token(&raw_token);
        assert_eq!(stored_hash, verify_lookup_hash);

        // The unprefixed suffix must NOT be what got hashed — this is
        // exactly the value the original bug stored instead.
        let raw_suffix = raw_token.strip_prefix("agt_").unwrap();
        assert_ne!(stored_hash, hash_token(raw_suffix));
    }
}
