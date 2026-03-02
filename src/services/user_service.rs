use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;

use crate::config::Config;
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::user::{NewUser, User};
use crate::schema::users::dsl::*;
use crate::services::admin_service;

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
) -> Result<User, AppError> {
    let mut conn = pool.get()?;
    let existing = users.find(user_id).first::<User>(&mut conn).optional()?;
    let existing = existing.ok_or(AppError::NotFound)?;

    let username_value = next_username.unwrap_or(existing.username);
    let avatar_value = match next_avatar_base64 {
        Some(value) => value,
        None => existing.avatar_base64,
    };

    diesel::update(users.find(user_id))
        .set((username.eq(username_value), avatar_base64.eq(avatar_value)))
        .get_result::<User>(&mut conn)
        .map_err(|e| match e {
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                _,
            ) => AppError::Conflict("Username already taken".into()),
            other => AppError::Database(other),
        })
}
