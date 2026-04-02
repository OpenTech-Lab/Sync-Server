use chrono::Utc;
use diesel::prelude::*;
use diesel::PgConnection;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::message::{Message, NewMessage};
use crate::schema::messages::dsl::*;
use crate::schema::user_blocks;
use crate::services::moderation_service;

/// Persist a new message and return the saved record.
pub fn send_message(
    pool: &Pool,
    sender: Uuid,
    recipient: Uuid,
    body: String,
) -> Result<Message, AppError> {
    let mut conn = pool.get()?;
    insert_message_conn(&mut conn, sender, recipient, &body).map_err(AppError::Database)
}

pub(crate) fn insert_message_conn(
    conn: &mut PgConnection,
    sender: Uuid,
    recipient: Uuid,
    body: &str,
) -> Result<Message, diesel::result::Error> {
    let new_msg = NewMessage {
        id: Uuid::new_v4(),
        sender_id: sender,
        recipient_id: recipient,
        content: body.to_string(),
    };
    diesel::insert_into(crate::schema::messages::table)
        .values(&new_msg)
        .get_result::<Message>(conn)
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
    if moderation_service::is_block_active_between(pool, user_a, user_b)? {
        return Ok(Vec::new());
    }

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
    if moderation_service::is_block_active_between(pool, viewer, partner)? {
        return Ok(0);
    }

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
    let mut conn = pool.get()?;
    let blocked_partner_ids = blocked_partner_ids_conn(&mut conn, viewer)?;
    let mut query = messages
        .filter(recipient_id.eq(viewer))
        .filter(read_at.is_null())
        .filter(deleted_at.is_null())
        .into_boxed();
    if !blocked_partner_ids.is_empty() {
        query = query.filter(sender_id.ne_all(blocked_partner_ids));
    }

    let rows = query.select(sender_id).load::<Uuid>(&mut conn)?;
    let mut counts = HashMap::<Uuid, i64>::new();
    for sender in rows {
        *counts.entry(sender).or_insert(0) += 1;
    }
    Ok(counts)
}

fn blocked_partner_ids_conn(conn: &mut PgConnection, viewer: Uuid) -> QueryResult<HashSet<Uuid>> {
    let pairs = user_blocks::table
        .filter(
            user_blocks::blocker_user_id
                .eq(viewer)
                .or(user_blocks::blocked_user_id.eq(viewer)),
        )
        .select((user_blocks::blocker_user_id, user_blocks::blocked_user_id))
        .load::<(Uuid, Uuid)>(conn)?;

    Ok(pairs
        .into_iter()
        .map(|(blocker_id, blocked_id)| {
            if blocker_id == viewer {
                blocked_id
            } else {
                blocker_id
            }
        })
        .collect())
}
