use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::stickers;

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Serialize)]
#[diesel(table_name = stickers)]
pub struct Sticker {
    pub id: Uuid,
    pub uploader_id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub content_base64: String,
    pub size_bytes: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = stickers)]
pub struct NewSticker {
    pub id: Uuid,
    pub uploader_id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub content_base64: String,
    pub size_bytes: i32,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct StickerListItem {
    pub id: Uuid,
    pub uploader_id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct StickerDetail {
    pub id: Uuid,
    pub uploader_id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: i32,
    pub status: String,
    pub content_base64: String,
    pub created_at: DateTime<Utc>,
}

impl From<Sticker> for StickerListItem {
    fn from(value: Sticker) -> Self {
        Self {
            id: value.id,
            uploader_id: value.uploader_id,
            name: value.name,
            mime_type: value.mime_type,
            size_bytes: value.size_bytes,
            status: value.status,
            created_at: value.created_at,
        }
    }
}

impl From<Sticker> for StickerDetail {
    fn from(value: Sticker) -> Self {
        Self {
            id: value.id,
            uploader_id: value.uploader_id,
            name: value.name,
            mime_type: value.mime_type,
            size_bytes: value.size_bytes,
            status: value.status,
            content_base64: value.content_base64,
            created_at: value.created_at,
        }
    }
}
