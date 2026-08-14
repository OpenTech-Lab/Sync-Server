use chrono::Utc;
use diesel::prelude::*;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::call_record::{CallRecord, NewCallRecord};
use crate::schema::call_records;

pub fn create(
    pool: &Pool,
    caller_id: Uuid,
    callee_id: Uuid,
    call_type: &str,
) -> Result<CallRecord, AppError> {
    let mut conn = pool.get()?;
    let record = NewCallRecord {
        id: Uuid::new_v4(),
        caller_id,
        callee_id,
        call_type: call_type.to_string(),
        status: "initiated".to_string(),
    };
    Ok(diesel::insert_into(call_records::table)
        .values(&record)
        .get_result(&mut conn)?)
}

pub fn update_status(pool: &Pool, record_id: Uuid, new_status: &str) -> Result<(), AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();
    match new_status {
        "answered" => {
            diesel::update(call_records::table.find(record_id))
                .set((
                    call_records::status.eq("answered"),
                    call_records::answered_at.eq(Some(now)),
                ))
                .execute(&mut conn)?;
        }
        "ended" | "rejected" | "missed" | "failed" => {
            diesel::update(call_records::table.find(record_id))
                .set((
                    call_records::status.eq(new_status),
                    call_records::ended_at.eq(Some(now)),
                ))
                .execute(&mut conn)?;
        }
        _ => {}
    }
    Ok(())
}

/// Looks up a call record by id.
pub fn find(pool: &Pool, call_id: Uuid) -> Result<Option<CallRecord>, AppError> {
    let mut conn = pool.get()?;
    call_records::table
        .find(call_id)
        .first(&mut conn)
        .optional()
        .map_err(Into::into)
}

/// Returns true if `user_id` is the caller or callee on `call_id`.
///
/// Returns `Ok(false)` (not an error) if the call record doesn't exist, so
/// callers can treat "no such call" and "not a participant" the same way.
pub fn is_call_participant(pool: &Pool, call_id: Uuid, user_id: Uuid) -> Result<bool, AppError> {
    let mut conn = pool.get()?;
    let record: Option<CallRecord> = call_records::table
        .find(call_id)
        .first(&mut conn)
        .optional()?;
    Ok(record.is_some_and(|r| r.caller_id == user_id || r.callee_id == user_id))
}

pub fn history_for_user(
    pool: &Pool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<CallRecord>, AppError> {
    let mut conn = pool.get()?;
    call_records::table
        .filter(
            call_records::caller_id
                .eq(user_id)
                .or(call_records::callee_id.eq(user_id)),
        )
        .order(call_records::started_at.desc())
        .limit(limit)
        .load(&mut conn)
        .map_err(Into::into)
}
