use chrono::{DateTime, Utc};
use diesel::dsl::count_star;
use diesel::prelude::*;
use diesel::PgConnection;
use uuid::Uuid;

use crate::db::Pool;
use crate::errors::AppError;
use crate::models::room::{NewRoom, NewRoomMember, NewRoomMessage, Room, RoomMember, RoomMessage};
use crate::schema::{room_members, room_messages, rooms, users};

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoomMemberProfile {
    pub user_id: Uuid,
    pub username: String,
    pub avatar_base64: Option<String>,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoomSummary {
    pub id: Uuid,
    pub name: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub member_count: i64,
    pub unread_count: i64,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoomDetail {
    pub id: Uuid,
    pub name: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub member_count: i64,
    pub unread_count: i64,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub members: Vec<RoomMemberProfile>,
}

pub fn create_room(
    pool: &Pool,
    creator_id: Uuid,
    name: &str,
    member_ids: &[Uuid],
) -> Result<RoomDetail, AppError> {
    let mut conn = pool.get()?;
    conn.transaction::<RoomDetail, AppError, _>(|conn| {
        let room_name = normalize_room_name(name)?;
        let member_ids = normalize_member_ids(creator_id, member_ids);
        ensure_users_exist_conn(conn, &member_ids)?;

        let room = diesel::insert_into(rooms::table)
            .values(&NewRoom {
                id: Uuid::new_v4(),
                name: room_name,
                created_by: creator_id,
            })
            .get_result::<Room>(conn)?;

        let now = Utc::now();
        let records = member_ids
            .iter()
            .map(|member_id| NewRoomMember {
                room_id: room.id,
                user_id: *member_id,
                role: if *member_id == creator_id {
                    "owner".to_string()
                } else {
                    "member".to_string()
                },
                last_read_at: Some(now),
            })
            .collect::<Vec<_>>();

        diesel::insert_into(room_members::table)
            .values(&records)
            .execute(conn)?;

        room_detail_conn(conn, &room, creator_id)
    })
}

pub fn list_rooms(pool: &Pool, viewer_id: Uuid) -> Result<Vec<RoomSummary>, AppError> {
    let mut conn = pool.get()?;
    let memberships = room_members::table
        .filter(room_members::user_id.eq(viewer_id))
        .order(room_members::joined_at.desc())
        .load::<RoomMember>(&mut conn)?;

    if memberships.is_empty() {
        return Ok(Vec::new());
    }

    let room_ids = memberships
        .iter()
        .map(|member| member.room_id)
        .collect::<Vec<_>>();
    let rooms_by_id = rooms::table
        .filter(rooms::id.eq_any(&room_ids))
        .load::<Room>(&mut conn)?
        .into_iter()
        .map(|room| (room.id, room))
        .collect::<std::collections::HashMap<_, _>>();

    let mut summaries = Vec::new();
    for membership in memberships {
        if let Some(room) = rooms_by_id.get(&membership.room_id) {
            summaries.push(room_summary_conn(&mut conn, room, &membership, viewer_id)?);
        }
    }

    summaries.sort_by(|a, b| {
        let a_ts = a.last_message_at.unwrap_or(a.updated_at);
        let b_ts = b.last_message_at.unwrap_or(b.updated_at);
        b_ts.cmp(&a_ts)
    });
    Ok(summaries)
}

pub fn get_room(pool: &Pool, room_id: Uuid, viewer_id: Uuid) -> Result<RoomDetail, AppError> {
    let mut conn = pool.get()?;
    let room = ensure_room_member_conn(&mut conn, room_id, viewer_id)?;
    room_detail_conn(&mut conn, &room, viewer_id)
}

pub fn get_room_messages(
    pool: &Pool,
    room_id: Uuid,
    viewer_id: Uuid,
    before_id: Option<Uuid>,
    limit: u8,
) -> Result<Vec<RoomMessage>, AppError> {
    let mut conn = pool.get()?;
    ensure_room_member_conn(&mut conn, room_id, viewer_id)?;

    let limit = limit.min(100) as i64;
    let base = room_messages::table.filter(room_messages::room_id.eq(room_id));

    let messages = if let Some(cursor_id) = before_id {
        let cursor = room_messages::table
            .find(cursor_id)
            .first::<RoomMessage>(&mut conn)
            .optional()?
            .ok_or(AppError::NotFound)?;
        if cursor.room_id != room_id {
            return Err(AppError::NotFound);
        }
        base.filter(
            room_messages::created_at
                .lt(cursor.created_at)
                .or(room_messages::created_at
                    .eq(cursor.created_at)
                    .and(room_messages::id.lt(cursor_id))),
        )
        .order((room_messages::created_at.desc(), room_messages::id.desc()))
        .limit(limit)
        .load::<RoomMessage>(&mut conn)?
    } else {
        base.order((room_messages::created_at.desc(), room_messages::id.desc()))
            .limit(limit)
            .load::<RoomMessage>(&mut conn)?
    };

    Ok(messages)
}

pub fn send_room_message(
    pool: &Pool,
    room_id: Uuid,
    sender_id: Uuid,
    body: &str,
) -> Result<RoomMessage, AppError> {
    let mut conn = pool.get()?;
    conn.transaction::<RoomMessage, AppError, _>(|conn| {
        ensure_room_member_conn(conn, room_id, sender_id)?;
        let trimmed = body.trim();
        if trimmed.is_empty() {
            return Err(AppError::BadRequest(
                "Message content cannot be empty".into(),
            ));
        }

        let message = diesel::insert_into(room_messages::table)
            .values(&NewRoomMessage {
                id: Uuid::new_v4(),
                room_id,
                sender_id,
                content: trimmed.to_string(),
            })
            .get_result::<RoomMessage>(conn)?;

        let now = Utc::now();
        diesel::update(rooms::table.find(room_id))
            .set(rooms::updated_at.eq(now))
            .execute(conn)?;
        diesel::update(
            room_members::table
                .filter(room_members::room_id.eq(room_id))
                .filter(room_members::user_id.eq(sender_id)),
        )
        .set(room_members::last_read_at.eq(Some(now)))
        .execute(conn)?;

        Ok(message)
    })
}

pub fn mark_room_read(pool: &Pool, room_id: Uuid, viewer_id: Uuid) -> Result<i64, AppError> {
    let mut conn = pool.get()?;
    conn.transaction::<i64, AppError, _>(|conn| {
        let membership = room_members::table
            .filter(room_members::room_id.eq(room_id))
            .filter(room_members::user_id.eq(viewer_id))
            .first::<RoomMember>(conn)
            .optional()?;
        let Some(membership) = membership else {
            return Err(room_access_error(conn, room_id));
        };

        let unread = room_unread_count_conn(conn, room_id, viewer_id, membership.last_read_at)?;
        diesel::update(
            room_members::table
                .filter(room_members::room_id.eq(room_id))
                .filter(room_members::user_id.eq(viewer_id)),
        )
        .set(room_members::last_read_at.eq(Some(Utc::now())))
        .execute(conn)?;
        Ok(unread)
    })
}

pub fn list_room_member_ids(
    pool: &Pool,
    room_id: Uuid,
    viewer_id: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    let mut conn = pool.get()?;
    ensure_room_member_conn(&mut conn, room_id, viewer_id)?;
    room_members::table
        .filter(room_members::room_id.eq(room_id))
        .select(room_members::user_id)
        .load::<Uuid>(&mut conn)
        .map_err(AppError::from)
}

fn normalize_room_name(raw: &str) -> Result<String, AppError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(AppError::BadRequest("Room name cannot be empty".into()));
    }
    if normalized.chars().count() > 80 {
        return Err(AppError::BadRequest(
            "Room name must be <= 80 characters".into(),
        ));
    }
    Ok(normalized.to_string())
}

fn normalize_member_ids(creator_id: Uuid, member_ids: &[Uuid]) -> Vec<Uuid> {
    let mut deduped = std::collections::BTreeSet::new();
    deduped.insert(creator_id);
    for member_id in member_ids {
        deduped.insert(*member_id);
    }
    deduped.into_iter().collect::<Vec<_>>()
}

fn ensure_users_exist_conn(conn: &mut PgConnection, member_ids: &[Uuid]) -> Result<(), AppError> {
    let found = users::table
        .filter(users::id.eq_any(member_ids))
        .select(users::id)
        .load::<Uuid>(conn)?;
    if found.len() != member_ids.len() {
        return Err(AppError::BadRequest(
            "One or more room members do not exist".into(),
        ));
    }
    Ok(())
}

fn ensure_room_member_conn(
    conn: &mut PgConnection,
    room_id: Uuid,
    viewer_id: Uuid,
) -> Result<Room, AppError> {
    let room = rooms::table.find(room_id).first::<Room>(conn).optional()?;
    let Some(room) = room else {
        return Err(AppError::NotFound);
    };
    let membership = room_members::table
        .filter(room_members::room_id.eq(room_id))
        .filter(room_members::user_id.eq(viewer_id))
        .select(room_members::room_id)
        .first::<Uuid>(conn)
        .optional()?;
    if membership.is_none() {
        return Err(AppError::Forbidden);
    }
    Ok(room)
}

fn room_access_error(conn: &mut PgConnection, room_id: Uuid) -> AppError {
    match rooms::table.find(room_id).first::<Room>(conn).optional() {
        Ok(Some(_)) => AppError::Forbidden,
        Ok(None) => AppError::NotFound,
        Err(error) => AppError::Database(error),
    }
}

fn room_detail_conn(
    conn: &mut PgConnection,
    room: &Room,
    viewer_id: Uuid,
) -> Result<RoomDetail, AppError> {
    let membership = room_members::table
        .filter(room_members::room_id.eq(room.id))
        .filter(room_members::user_id.eq(viewer_id))
        .first::<RoomMember>(conn)?;
    let summary = room_summary_conn(conn, room, &membership, viewer_id)?;
    let members = room_member_profiles_conn(conn, room.id)?;
    Ok(RoomDetail {
        id: summary.id,
        name: summary.name,
        created_by: summary.created_by,
        created_at: summary.created_at,
        updated_at: summary.updated_at,
        member_count: summary.member_count,
        unread_count: summary.unread_count,
        last_message_preview: summary.last_message_preview,
        last_message_at: summary.last_message_at,
        members,
    })
}

fn room_summary_conn(
    conn: &mut PgConnection,
    room: &Room,
    membership: &RoomMember,
    viewer_id: Uuid,
) -> Result<RoomSummary, AppError> {
    let member_count = room_members::table
        .filter(room_members::room_id.eq(room.id))
        .select(count_star())
        .first::<i64>(conn)?;
    let latest_message = room_messages::table
        .filter(room_messages::room_id.eq(room.id))
        .order((room_messages::created_at.desc(), room_messages::id.desc()))
        .first::<RoomMessage>(conn)
        .optional()?;
    let unread_count = room_unread_count_conn(conn, room.id, viewer_id, membership.last_read_at)?;
    Ok(RoomSummary {
        id: room.id,
        name: room.name.clone(),
        created_by: room.created_by,
        created_at: room.created_at,
        updated_at: room.updated_at,
        member_count,
        unread_count,
        last_message_preview: latest_message
            .as_ref()
            .map(|message| truncate_preview(&message.content)),
        last_message_at: latest_message.map(|message| message.created_at),
    })
}

fn room_unread_count_conn(
    conn: &mut PgConnection,
    room_id_value: Uuid,
    viewer_id: Uuid,
    last_read_at: Option<DateTime<Utc>>,
) -> QueryResult<i64> {
    let mut query = room_messages::table
        .filter(room_messages::room_id.eq(room_id_value))
        .filter(room_messages::sender_id.ne(viewer_id))
        .into_boxed();
    if let Some(last_read_at) = last_read_at {
        query = query.filter(room_messages::created_at.gt(last_read_at));
    }
    query.select(count_star()).first::<i64>(conn)
}

fn room_member_profiles_conn(
    conn: &mut PgConnection,
    room_id_value: Uuid,
) -> Result<Vec<RoomMemberProfile>, AppError> {
    let memberships = room_members::table
        .filter(room_members::room_id.eq(room_id_value))
        .order(room_members::joined_at.asc())
        .load::<RoomMember>(conn)?;
    if memberships.is_empty() {
        return Ok(Vec::new());
    }

    let users = users::table
        .filter(
            users::id.eq_any(
                memberships
                    .iter()
                    .map(|member| member.user_id)
                    .collect::<Vec<_>>(),
            ),
        )
        .select((users::id, users::username, users::avatar_base64))
        .load::<(Uuid, String, Option<String>)>(conn)?
        .into_iter()
        .map(|(user_id, username, avatar_base64)| (user_id, (username, avatar_base64)))
        .collect::<std::collections::HashMap<_, _>>();

    let mut profiles = Vec::with_capacity(memberships.len());
    for membership in memberships {
        let Some((username, avatar_base64)) = users.get(&membership.user_id) else {
            continue;
        };
        profiles.push(RoomMemberProfile {
            user_id: membership.user_id,
            username: username.clone(),
            avatar_base64: avatar_base64.clone(),
            role: membership.role,
            joined_at: membership.joined_at,
        });
    }
    Ok(profiles)
}

fn truncate_preview(content: &str) -> String {
    const MAX_CHARS: usize = 120;
    let trimmed = content.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    let mut value = String::new();
    for ch in trimmed.chars().take(MAX_CHARS) {
        value.push(ch);
    }
    value.push_str("...");
    value
}
