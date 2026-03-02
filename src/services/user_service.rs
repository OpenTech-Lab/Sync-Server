use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::user::{NewUser, User};
use crate::schema::users::dsl::*;

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

/// Update `last_seen_at` for a user to now.
pub fn update_last_seen(pool: &Pool, user_id: Uuid) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    diesel::update(users.find(user_id))
        .set(last_seen_at.eq(Some(Utc::now())))
        .execute(&mut conn)?;
    Ok(())
}
