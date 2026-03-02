use chrono::Utc;
use diesel::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::message::{Message, NewMessage};
use crate::schema::messages::dsl::*;

/// Persist a new message and return the saved record.
pub fn send_message(
    pool: &Pool,
    sender: Uuid,
    recipient: Uuid,
    body: String,
) -> Result<Message, AppError> {
    let mut conn = pool.get()?;
    let new_msg = NewMessage {
        id: Uuid::new_v4(),
        sender_id: sender,
        recipient_id: recipient,
        content: body,
    };
    diesel::insert_into(crate::schema::messages::table)
        .values(&new_msg)
        .get_result::<Message>(&mut conn)
        .map_err(AppError::Database)
}

/// Keyset-paginated conversation between two users.
///
/// Returns up to `limit` messages (max 100) ordered newest-first.
/// Pass `before_id` (a message UUID) to get the page preceding that message.
pub fn get_conversation(
    pool: &Pool,
    user_a: Uuid,
    user_b: Uuid,
    before_id: Option<Uuid>,
    limit: u8,
) -> Result<Vec<Message>, AppError> {
    let limit = limit.min(100) as i64;
    let mut conn = pool.get()?;

    // Base filter: messages between the two users in either direction.
    let base = messages
        .filter(
            (sender_id.eq(user_a).and(recipient_id.eq(user_b)))
                .or(sender_id.eq(user_b).and(recipient_id.eq(user_a))),
        )
        .filter(deleted_at.is_null());

    let result = if let Some(cursor_id) = before_id {
        // Fetch cursor message's created_at for keyset comparison
        let cursor_msg = crate::schema::messages::table
            .find(cursor_id)
            .first::<Message>(&mut conn)
            .optional()?
            .ok_or_else(|| AppError::NotFound)?;

        base.filter(
            created_at
                .lt(cursor_msg.created_at)
                .or(created_at.eq(cursor_msg.created_at).and(id.lt(cursor_id))),
        )
        .order((created_at.desc(), id.desc()))
        .limit(limit)
        .load::<Message>(&mut conn)?
    } else {
        base.order((created_at.desc(), id.desc()))
            .limit(limit)
            .load::<Message>(&mut conn)?
    };

    Ok(result)
}

/// Mark all unread messages from `partner` to `viewer` as delivered and read.
pub fn mark_read(pool: &Pool, viewer: Uuid, partner: Uuid) -> Result<usize, AppError> {
    let mut conn = pool.get()?;
    let now = Utc::now();

    // Set delivered_at only for messages not yet delivered (COALESCE-like behaviour
    // without raw SQL — two targeted updates instead of one to avoid Diesel type friction).
    diesel::update(
        messages
            .filter(sender_id.eq(partner))
            .filter(recipient_id.eq(viewer))
            .filter(read_at.is_null())
            .filter(delivered_at.is_null()),
    )
    .set(delivered_at.eq(Some(now)))
    .execute(&mut conn)?;

    // Mark as read — return the number of rows affected.
    let count = diesel::update(
        messages
            .filter(sender_id.eq(partner))
            .filter(recipient_id.eq(viewer))
            .filter(read_at.is_null()),
    )
    .set(read_at.eq(Some(now)))
    .execute(&mut conn)?;

    Ok(count)
}

/// Return a map of `partner_user_id → unread_count` for the given user.
pub fn unread_counts(pool: &Pool, viewer: Uuid) -> Result<HashMap<Uuid, i64>, AppError> {
    use crate::schema::messages;
    use diesel::dsl::count_star;

    let mut conn = pool.get()?;

    let rows: Vec<(Uuid, i64)> = messages::table
        .filter(messages::recipient_id.eq(viewer))
        .filter(messages::read_at.is_null())
        .filter(messages::deleted_at.is_null())
        .group_by(messages::sender_id)
        .select((messages::sender_id, count_star()))
        .load(&mut conn)?;

    Ok(rows.into_iter().collect())
}
