use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use uuid::Uuid;

use crate::schema::{room_members, room_messages, rooms};

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize)]
#[diesel(table_name = rooms)]
pub struct Room {
    pub id: Uuid,
    pub name: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = rooms)]
pub struct NewRoom {
    pub id: Uuid,
    pub name: String,
    pub created_by: Uuid,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Associations, Serialize)]
#[diesel(table_name = room_members)]
#[diesel(primary_key(room_id, user_id))]
#[diesel(belongs_to(Room))]
pub struct RoomMember {
    pub room_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = room_members)]
pub struct NewRoomMember {
    pub room_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub last_read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Associations, Serialize)]
#[diesel(table_name = room_messages)]
#[diesel(belongs_to(Room))]
pub struct RoomMessage {
    pub id: Uuid,
    pub room_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = room_messages)]
pub struct NewRoomMessage {
    pub id: Uuid,
    pub room_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
}
