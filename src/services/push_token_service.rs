use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::device_push_token::{DevicePushToken, NewDevicePushToken};
use crate::schema::device_push_tokens::dsl as dpt_dsl;

pub fn upsert_token(
    pool: &Pool,
    user_id: Uuid,
    platform: &str,
    token: &str,
) -> Result<DevicePushToken, AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();

    diesel::insert_into(crate::schema::device_push_tokens::table)
        .values(NewDevicePushToken {
            id: Uuid::new_v4(),
            user_id,
            platform: platform.to_string(),
            token: token.to_string(),
            last_seen_at: Some(now),
        })
        .on_conflict(dpt_dsl::token)
        .do_update()
        .set((
            dpt_dsl::user_id.eq(user_id),
            dpt_dsl::platform.eq(platform),
            dpt_dsl::updated_at.eq(now),
            dpt_dsl::last_seen_at.eq(Some(now)),
        ))
        .execute(&mut conn)?;

    dpt_dsl::device_push_tokens
        .filter(dpt_dsl::token.eq(token))
        .select(DevicePushToken::as_select())
        .first::<DevicePushToken>(&mut conn)
        .map_err(AppError::from)
}

pub fn unregister_token(pool: &Pool, user_id: Uuid, token: &str) -> Result<usize, AppError> {
    let mut conn = pool.get()?;
    diesel::delete(
        dpt_dsl::device_push_tokens
            .filter(dpt_dsl::user_id.eq(user_id))
            .filter(dpt_dsl::token.eq(token)),
    )
    .execute(&mut conn)
    .map_err(AppError::from)
}

pub fn list_tokens_for_user(pool: &Pool, user_id: Uuid) -> Result<Vec<DevicePushToken>, AppError> {
    let mut conn = pool.get()?;
    dpt_dsl::device_push_tokens
        .filter(dpt_dsl::user_id.eq(user_id))
        .select(DevicePushToken::as_select())
        .load::<DevicePushToken>(&mut conn)
        .map_err(AppError::from)
}
